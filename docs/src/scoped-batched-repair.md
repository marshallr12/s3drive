# Scoped source operations with accepted upload batching

Declared 2026-10-19. Isolated repair phase from1ad9dc6/schema36/all329. Original
scoped-operation source and two repair gates retained upload timeouts; none of
those sources is accepted. The independent batch source2fc45fed subsequently
passed its complete local and own hosted/platform/artifact gates and is integrated
into main5995ea5. Its artifacts do not accept this combined source.

Apply only the five accepted runtime deltas to the scoped repair using a three-way
merge. Preserve scoped token semantics, query validation, all old assertions and
the scoped SDK cycle. Remove the two-family SDK fixture admission semaphore and
its600-second queue budget; use the repository's ordinary four test threads.
Production30-second operations,180-second child deadlines and all internal race,
physical recovery, fresh authorization and write-policy checks remain unchanged.
No schema/dependency change beyond the existing scoped schema36 is introduced.

Ownership: the five upload-batch runtime paths; removal of fixture admission;
repair documentation, frozen inputs and verification records. Source guard and
managed-reader work remains isolated. Do not change those frozen inputs while
their component runner is live; share only the build target sequentially.

Done criteria: clean three-way runtime merge with all scoped+batch assertions;
exact six history/six base HEAD checks for three objects and closing-auth denial;
focused scoped cases and complete local gate at ordinary concurrency; this exact
combined source's own Linux/Windows/native/source/artifact acceptance. Only then
promote schema36 and integrate the subsequent read-pin/reader work. No old source's
passing job or stand-alone case substitutes for this source's full gate. No native
chunk writes, reclamation, Wasabi mutations or physical session claims follow.
