use super::*;
use crate::{
    probe::tests::{Mode, Server},
    transport::Session,
    uploads::{Executor, Phase},
};
use s3drive_state::{cache, journal, worker::Storage};

// Synthetic local model evidence is deliberately not provider acceptance.
pub(in crate::uploads) fn documents(session: &Session) -> (String, String, String) {
    let plan = probe::Plan::prepare(&session.reader.source).unwrap();
    let observed = now().unwrap();
    let mut report = format!(
        "version = 1\nprobe_version = 'conditional-put-v1'\nobserved_unix_seconds = {observed}\nplan_digest = '{}'\nsource_fingerprint = '{}'\ncredential_generation = '{}'\npassed = true\nerrors = []\nremaining_keys = []\n",
        plan.digest().unwrap(),
        session.reader.configuration_fingerprint(),
        session.binding,
    );
    for name in probe::CHECK_NAMES {
        report.push_str(&format!(
            "\n[[checks]]\nname = '{name}'\npassed = true\ndetail = 'local fixture'\n"
        ));
    }
    let policy = policy_text(&session.reader.source, &report);
    (policy, plan.to_toml().unwrap(), report)
}
fn policy_text(source: &Source, report: &str) -> String {
    format!(
        "version = 1\nsource_fingerprint = '{}'\nreport_sha256 = '{}'\nwritable_prefix = ''\nmax_object_bytes = 8388608\noperations = 'create-and-replace'\nobject_metadata='s3drive-markers-only'\nevidence_max_age_seconds = 86400\n",
        source_fingerprint(source),
        transport::hex(Sha256::digest(report.as_bytes()).as_slice()),
    )
}
pub(in crate::uploads) fn fixture_policy(session: &Session) -> WritePolicy {
    let (policy, plan, report) = documents(session);
    WritePolicy::from_toml(&session.reader.source, &policy, &plan, &report).unwrap()
}
async fn storage(path: &std::path::Path) -> Storage {
    Storage::open(path, cache::Options::default(), journal::Options::default())
        .await
        .unwrap()
}
fn failure(error: &crate::uploads::Error, expected: Failure) {
    assert_eq!(error.cause().downcast_ref::<Failure>(), Some(&expected));
}

#[tokio::test]
async fn local_target_preview_enforces_scope_size_age_and_source_without_s3() {
    let server = Server::new(Mode::Normal).await;
    let session = server.session().await;
    let (policy, plan, report) = documents(&session);
    let source = server.source();
    let scoped = policy.replace("writable_prefix = ''", "writable_prefix = 'managed/'");
    let evidence = WritePolicy::from_toml(&source, &scoped, &plan, &report).unwrap();
    assert_eq!(
        evidence.check_target_scope(&source, "managed/v1/object", 0),
        Ok(())
    );
    assert_eq!(
        evidence.check_target_scope(&source, "outside", 0),
        Err(Failure::PolicyScope)
    );
    assert_eq!(
        evidence.check_target_scope(&source, "managed/object", 8_388_609),
        Err(Failure::PolicySize)
    );
    let mut changed = source.clone();
    changed.profile = Some("another-profile".into());
    assert_eq!(
        evidence.check_target_scope(&changed, "managed/object", 0),
        Err(Failure::BindingMismatch)
    );
    let mut expired = evidence;
    expired.expires = 0;
    assert_eq!(
        expired.check_target_scope(&source, "managed/object", 0),
        Err(Failure::PolicyExpired)
    );
    assert_eq!(
        server.count("PUT") + server.count("GET") + server.count("HEAD") + server.count("DELETE"),
        0
    );
}

#[tokio::test]
async fn strict_evidence_requires_every_exact_check_and_a_matching_complete_plan() {
    let server = Server::new(Mode::Normal).await;
    let session = server.session().await;
    let (policy, plan, report) = documents(&session);
    let source = server.source();
    let parse = |p: &str, plan: &str, r: &str| WritePolicy::from_toml(&source, p, plan, r);
    assert!(parse(&policy, &plan, &report).is_ok());
    // Even whitespace changes select a different exact report.
    assert!(matches!(
        parse(&policy, &plan, &format!("{report}\n")),
        Err(SetupError::Evidence)
    ));
    let original: toml::Value = toml::from_str(&report).unwrap();
    for case in 0..15 {
        let mut value = original.clone();
        match case {
            0 => {
                value["passed"] = false.into();
            }
            1 => {
                value["errors"] = toml::Value::Array(vec!["untrusted detail".into()]);
            }
            2 => {
                value["remaining_keys"] = toml::Value::Array(vec!["leftover".into()]);
            }
            3 => {
                value["version"] = 2.into();
            }
            4 => {
                value["probe_version"] = "conditional-put-v2".into();
            }
            5 => {
                value["credential_generation"] = "invalid".into();
            }
            6 => {
                value["plan_digest"] = "00".repeat(32).into();
            }
            7 => {
                value["source_fingerprint"] = "00".repeat(32).into();
            }
            8 => {
                value["observed_unix_seconds"] = 1.into();
            }
            9 => {
                value["observed_unix_seconds"] = (now().unwrap() as i64 + 3600).into();
            }
            10 => {
                value["checks"].as_array_mut().unwrap().pop();
            }
            11 => {
                value["checks"][1] = value["checks"][0].clone();
            }
            12 => {
                value["checks"][0]["passed"] = false.into();
            }
            13 => {
                value["checks"][0]
                    .as_table_mut()
                    .unwrap()
                    .insert("unknown".into(), "forbidden".into());
            }
            _ => {
                value
                    .as_table_mut()
                    .unwrap()
                    .insert("unknown".into(), "forbidden".into());
            }
        }
        let report = toml::to_string(&value).unwrap();
        let policy = policy_text(&source, &report);
        assert!(
            matches!(parse(&policy, &plan, &report), Err(SetupError::Evidence)),
            "case {case}"
        );
    }
    let wrong_plan = plan.replace("max_put_attempts = 14", "max_put_attempts = 15");
    assert!(matches!(
        parse(&policy, &wrong_plan, &report),
        Err(SetupError::Evidence)
    ));
    for bad in [
        "unknown = 'PRIVATE_MARKER'\n",
        "profile = 'PRIVATE_MARKER'\n",
    ] {
        let error = parse(&format!("{bad}{policy}"), &plan, &report).unwrap_err();
        assert!(!format!("{error:?} {error}").contains("PRIVATE_MARKER"));
    }
    for field in ["max_object_bytes", "evidence_max_age_seconds"] {
        let value: toml::Value = toml::from_str(&policy).unwrap();
        for number in [0, -1, i64::MAX] {
            let mut changed = value.clone();
            changed[field] = number.into();
            assert!(matches!(
                parse(&toml::to_string(&changed).unwrap(), &plan, &report),
                Err(SetupError::Policy)
            ));
        }
    }
    for prefix in [
        "docs",
        "/docs/",
        "docs/../",
        "docs//",
        "docs/\\bad/",
        "docs/\n/",
    ] {
        let mut value: toml::Value = toml::from_str(&policy).unwrap();
        value["writable_prefix"] = prefix.into();
        assert!(matches!(
            parse(&toml::to_string(&value).unwrap(), &plan, &report),
            Err(SetupError::Policy)
        ));
    }
    for (p, pl, r) in [
        ("x".repeat(MAX_TEXT + 1), plan.clone(), report.clone()),
        (policy.clone(), "x".repeat(MAX_TEXT + 1), report.clone()),
        (policy.clone(), plan.clone(), "x".repeat(MAX_TEXT + 1)),
    ] {
        assert!(parse(&p, &pl, &r).is_err());
    }
    assert_eq!(server.count("PUT"), 0);
    assert_eq!(server.count("HEAD"), 0);
}

#[tokio::test]
async fn factory_rejects_source_and_each_credential_change_without_s3_requests() {
    use aws_sdk_s3::config::{Credentials, SharedCredentialsProvider};
    let server = Server::new(Mode::Normal).await;
    let session = server.session().await;
    let temp = tempfile::tempdir().unwrap();
    let storage = storage(&temp.path().join("state")).await;
    let (policy, plan, report) = documents(&session);
    let parse = || WritePolicy::from_toml(&server.source(), &policy, &plan, &report).unwrap();
    for field in 0..6 {
        let mut source = server.source();
        match field {
            0 => source.alias.push('x'),
            1 => source.bucket.push('x'),
            2 => source.region.push('x'),
            3 => source.prefix.push_str("extra/"),
            4 => source.endpoint = Some("http://127.0.0.1:9".into()),
            _ => source.profile = Some("other-profile".into()),
        }
        let reader = S3Reader::with_credentials(
            source,
            session.reader.credentials.clone(),
            session.reader.deadline,
            3,
        )
        .unwrap();
        assert!(matches!(
            Executor::open(reader, storage.clone(), parse()).await,
            Err(SetupError::SourceMismatch)
        ));
    }
    for credentials in [
        Credentials::new(
            "OTHER",
            "TESTSECRET",
            Some("TESTTOKEN".into()),
            None,
            "test",
        ),
        Credentials::new(
            "TESTACCESS",
            "OTHER",
            Some("TESTTOKEN".into()),
            None,
            "test",
        ),
        Credentials::new(
            "TESTACCESS",
            "TESTSECRET",
            Some("OTHER".into()),
            None,
            "test",
        ),
    ] {
        let reader = S3Reader::with_credentials(
            server.source(),
            SharedCredentialsProvider::new(credentials),
            session.reader.deadline,
            3,
        )
        .unwrap();
        assert!(matches!(
            Executor::open(reader, storage.clone(), parse()).await,
            Err(SetupError::CredentialMismatch)
        ));
    }
    assert_eq!(
        (
            server.count("PUT"),
            server.count("HEAD"),
            server.count("GET")
        ),
        (0, 0, 0)
    );
    storage.close().await.unwrap();
}

#[tokio::test]
async fn explicit_prefix_size_and_create_only_limits_precede_send_permission() {
    let server = Server::new(Mode::Normal).await;
    let session = server.session().await;
    let (p, plan, report) = documents(&session);
    let policy = p
        .replace("writable_prefix = ''", "writable_prefix = 'docs/'")
        .replace("8388608", "16")
        .replace("create-and-replace", "create-only");
    let policy = WritePolicy::from_toml(&server.source(), &policy, &plan, &report).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let executor = Executor::open(
        server.reader(),
        storage(&temp.path().join("state")).await,
        policy,
    )
    .await
    .unwrap();
    for key in [
        "docs",
        "docs/",
        "docs2/file",
        "other/file",
        "docs/../file",
        "s3drive-conformance/test/file",
    ] {
        assert!(executor.target(key).is_err(), "{key}");
    }
    let key = "docs/日本語 %2F + café.txt";
    assert_eq!(executor.target(key).unwrap().object_key(), key);
    let outside = super::super::tests::stage(&executor, "other/file", None, b"outside").await;
    failure(
        &executor.submit(outside).await.unwrap_err(),
        Failure::PolicyScope,
    );
    assert!(
        executor
            .0
            .storage
            .uploads()
            .for_generation(outside)
            .await
            .unwrap()
            .is_none()
    );
    let large = super::super::tests::stage(&executor, "docs/large", None, &[1; 17]).await;
    failure(
        &executor.submit(large).await.unwrap_err(),
        Failure::PolicySize,
    );
    assert!(
        executor
            .0
            .storage
            .uploads()
            .for_generation(large)
            .await
            .unwrap()
            .is_none()
    );
    let first = super::super::tests::stage(&executor, key, None, b"first").await;
    assert_eq!(executor.submit(first).await.unwrap().phase, Phase::Verified);
    let second = super::super::tests::stage(&executor, key, Some(first), b"second").await;
    failure(
        &executor.submit(second).await.unwrap_err(),
        Failure::PolicyOperation,
    );
    let info = executor
        .0
        .storage
        .uploads()
        .for_generation(second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(info.phase, Phase::Prepared);
    executor
        .0
        .storage
        .uploads()
        .cancel_prepared(info.operation)
        .await
        .unwrap();
    assert_eq!(server.count("PUT"), 1);
    executor.0.storage.close().await.unwrap();
}

#[tokio::test]
async fn expired_or_narrowed_policy_recovers_claimed_attempt_without_another_put() {
    let server = Server::new(Mode::DropAfterCommit).await;
    let session = server.session().await;
    let (_, plan, report) = documents(&session);
    let temp = tempfile::tempdir().unwrap();
    let storage = storage(&temp.path().join("state")).await;
    let executor = Executor::open(server.reader(), storage.clone(), fixture_policy(&session))
        .await
        .unwrap();
    let first = super::super::tests::stage(&executor, "existing/file", None, b"committed").await;
    let info = storage.uploads().prepare(first).await.unwrap();
    let permit = storage
        .uploads()
        .claim(info.operation, info.target.clone())
        .await
        .unwrap();
    let intent = executor.intent(&info).await.unwrap();
    assert!(matches!(
        executor.0.session.put(&intent).await.unwrap(),
        transport::Outcome::Unknown(_)
    ));
    drop(permit);
    // Use old but structurally valid evidence, not an injected runtime clock.
    let mut plan: toml::Value = toml::from_str(&plan).unwrap();
    plan["created_unix_seconds"] = 1.into();
    let plan = toml::to_string(&plan).unwrap();
    let mut report: toml::Value = toml::from_str(&report).unwrap();
    report["plan_digest"] = probe::Plan::from_toml(&plan)
        .unwrap()
        .digest()
        .unwrap()
        .into();
    report["observed_unix_seconds"] = (now().unwrap() as i64 - 86401).into();
    let report = toml::to_string(&report).unwrap();
    let policy = policy_text(&server.source(), &report)
        .replace("writable_prefix = ''", "writable_prefix = 'narrow/'")
        .replace("8388608", "1");
    let expired = WritePolicy::from_toml(&server.source(), &policy, &plan, &report).unwrap();
    let recovery = Executor::open(server.reader(), storage.clone(), expired)
        .await
        .unwrap();
    assert_eq!(recovery.submit(first).await.unwrap().phase, Phase::Verified);
    assert_eq!(
        recovery.reconcile(first).await.unwrap().phase,
        Phase::Verified
    );
    let second = super::super::tests::stage(&recovery, "narrow/file", None, b"x").await;
    failure(
        &recovery.submit(second).await.unwrap_err(),
        Failure::PolicyExpired,
    );
    assert!(
        storage
            .uploads()
            .for_generation(second)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(server.count("PUT"), 1);
    let prepared = storage.uploads().prepare(second).await.unwrap();
    failure(
        &recovery.submit(second).await.unwrap_err(),
        Failure::PolicyExpired,
    );
    assert_eq!(
        recovery.reconcile(second).await.unwrap().phase,
        Phase::Prepared
    );
    storage
        .uploads()
        .cancel_prepared(prepared.operation)
        .await
        .unwrap();
    assert_eq!(
        storage.staging().read(first, 0, 16).await.unwrap(),
        b"committed"
    );
    storage.close().await.unwrap();
}
