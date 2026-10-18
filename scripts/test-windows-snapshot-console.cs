// Disposable ordinary-account acceptance helper. Never attach to or signal the
// runner console. Only a newly allocated, verified two-process console is used.
// Windows PowerShell5.1 compiles this test-only helper; no product dependency.
using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Text.RegularExpressions;
using System.Threading;

public static class S3DriveSnapshotConsoleFixture {
    private delegate bool Handler(uint control);
    private static readonly Handler OwnHandler = delegate(uint control) { return control == 0; };
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool AllocConsole();
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool FreeConsole();
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetConsoleCtrlHandler(Handler handler, bool add);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint GetConsoleProcessList([Out] uint[] processes, uint length);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GenerateConsoleCtrlEvent(uint control, uint group);

    private static void Require(bool condition, string message) {
        if (!condition) throw new InvalidOperationException(message);
    }
    private static bool OnlyConsole(int child, bool allowStarting = false) {
        var processes = new uint[4];
        uint count = GetConsoleProcessList(processes, (uint)processes.Length);
        uint self = (uint)Process.GetCurrentProcess().Id;
        if (child == 0) {
            Require(count == 1 && processes[0] == self, "Console was not isolated");
            return true;
        } else {
            if (allowStarting && count == 1 && processes[0] == self) return false;
            Require(count == 2 && ((processes[0] == self && processes[1] == (uint)child)
                || (processes[1] == self && processes[0] == (uint)child)), "Unexpected console participant");
            return true;
        }
    }
    private static string Quote(string text) {
        Require(text.IndexOf('"') < 0 && text.IndexOf('\0') < 0, "Invalid fixture argument");
        return "\"" + Regex.Replace(text, @"(\\+)$", "$1$1") + "\"";
    }
    private static Process Start(string root, string volume, string verb, string directory, string output) {
        var info = new ProcessStartInfo(Path.Combine(root, "s3drive.exe"));
        info.Arguments = "authority-snapshot " + verb + " --directory " + Quote(directory)
            + " --expect-volume " + Quote(volume) + " --name-policy portable-collision-v1";
        if (output != null) info.Arguments += " --output " + Quote(output);
        info.UseShellExecute = false;
        info.CreateNoWindow = false; // Inherit only this helper's new console.
        info.RedirectStandardInput = true;
        info.RedirectStandardOutput = true;
        info.RedirectStandardError = true;
        info.EnvironmentVariables["AWS_CONFIG_FILE"] = Path.Combine(root, "absent-config");
        info.EnvironmentVariables["AWS_SHARED_CREDENTIALS_FILE"] = Path.Combine(root, "absent-credentials");
        info.EnvironmentVariables["AWS_EC2_METADATA_DISABLED"] = "true";
        var child = Process.Start(info);
        child.StandardInput.Close();
        return child;
    }
    private static void Reap(Process child) {
        if (child == null) return;
        try {
            if (!child.HasExited) {
                child.Kill();
                Require(child.WaitForExit(20000), "Owned child did not stop");
            }
        } finally { child.Dispose(); }
    }
    private static string Digest(string path) {
        using (var file = File.OpenRead(path))
        using (var sha = SHA256.Create()) {
            return BitConverter.ToString(sha.ComputeHash(file)).Replace("-", "").ToLowerInvariant();
        }
    }
    private static void Refuses(string root, string volume, string verb, string directory, string output) {
        Process child = null;
        try {
            child = Start(root, volume, verb, directory, output);
            var stdout = child.StandardOutput.ReadToEndAsync();
            var stderr = child.StandardError.ReadToEndAsync();
            Require(child.WaitForExit(20000), "Refusal child deadline");
            Require(child.ExitCode != 0 && stdout.GetAwaiter().GetResult().Length == 0, "Partial or replacement reported success");
            Require(stderr.GetAwaiter().GetResult().Length > 0, "Refusal had no error");
        } finally { Reap(child); }
    }
    public static int Main(string[] args) {
        // The fixed result file contains only assertion classifications, never
        // exception text, paths, tokens, source records or provider settings.
        if (args.Length != 2) return 2;
        string root = args[0];
        string result = Path.Combine(root, "snapshot-console-result.json");
        Process child = null;
        bool allocated = false;
        bool owned = false;
        string stage = "guard";
        try {
            var identity = WindowsIdentity.GetCurrent();
            Require(!new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator), "Not ordinary account");
            Require(File.ReadAllText(Path.Combine(root, "snapshot-fixture.marker")) == identity.User.Value, "Not owned fixture");
            owned = true;
            Require(!File.Exists(result), "Result already exists");
            string volume = args[1];
            Require(volume.Length == 32 && Regex.IsMatch(volume, "^[0-9a-f]{32}$"), "Invalid volume");
            string source = Path.Combine(root, "authority-state");
            string output = Path.Combine(root, "authority-snapshot-cancel");
            Require(!Directory.Exists(output), "Output already exists");
            string database = Path.Combine(source, "metadata.sqlite3");
            Require(new FileInfo(database).Length >= 128L * 1024 * 1024, "Padding not checkpointed");
            string before = Digest(database); // All fixture SQLite connections are closed.
            // Detach this helper only. It must never attach to an existing PID.
            stage = "allocate-console";
            FreeConsole();
            Require(AllocConsole(), "Could not allocate private console");
            allocated = true;
            OnlyConsole(0);
            // Do not enable the inheritable ignore-Ctrl+C attribute. Only this
            // helper gets a handler; the actual Rust child handles its own event.
            Require(SetConsoleCtrlHandler(null, false), "Could not reset Ctrl+C processing");
            Require(SetConsoleCtrlHandler(OwnHandler, true), "Could not register helper handler");
            stage = "start-copy";
            child = Start(root, volume, "create", source, output);
            var stdout = child.StandardOutput.ReadToEndAsync();
            var stderr = child.StandardError.ReadToEndAsync();
            var deadline = Stopwatch.StartNew();
            bool sharedConsole = false;
            while (!Directory.Exists(output)) {
                Require(!child.HasExited, "Snapshot finished before cancellation boundary");
                Require(deadline.ElapsedMilliseconds < 20000, "Output creation deadline");
                // Source validation scans128MiB before output creation. Verify
                // console membership before waiting for the copying boundary.
                if (!sharedConsole) sharedConsole = OnlyConsole(child.Id, true);
                Thread.Sleep(1);
            }
            Require(!child.HasExited, "Snapshot finished before event");
            OnlyConsole(child.Id); // Refuse any unexpected participant before broadcast.
            stage = "signal-and-cancel";
            var cancel = Stopwatch.StartNew();
            Require(GenerateConsoleCtrlEvent(0, 0), "Could not signal private console");
            Require(child.WaitForExit(20000), "Cancellation child deadline");
            cancel.Stop();
            Require(child.ExitCode != 0 && stdout.GetAwaiter().GetResult().Length == 0, "Cancellation reported success");
            Require(stderr.GetAwaiter().GetResult().Contains("Cancelled"), "Cooperative cancellation not observed");
            Reap(child); child = null;
            Require(!File.Exists(Path.Combine(output, "snapshot.json")), "Completion marker exists");
            Require(Digest(database) == before, "Source changed after cancellation");
            stage = "partial-inspection";
            Refuses(root, volume, "inspect", output, null);
            stage = "same-destination-refusal";
            Refuses(root, volume, "create", source, output);
            Require(Digest(database) == before, "Source changed after refusal checks");
            OnlyConsole(0);
            File.WriteAllText(result, "{\"private_console_verified\":true,\"cooperative_cancel_observed\":true,\"success_stdout\":false,\"completion_manifest_absent\":true,\"source_sha256_preserved\":true,\"partial_inspection_refused\":true,\"same_destination_retry_refused\":true,\"restore_authorized\":false,\"cancel_to_exit_ms\":"
                + cancel.ElapsedMilliseconds.ToString(CultureInfo.InvariantCulture) + "}");
            return 0;
        } catch {
            if (owned && !File.Exists(result)) File.WriteAllText(result, "{\"acceptance_failed\":true,\"stage\":\"" + stage + "\"}");
            return 1;
        } finally {
            Reap(child);
            if (allocated) FreeConsole();
            GC.KeepAlive(OwnHandler);
        }
    }
}
