# Metered Lua patterns

`lua_patterns.c` contains the pattern section of Lua 5.4.7 `lstrlib.c` and its
`posrelatI` helper, under the original MIT license in `LICENSE.lua`.
Upstream: <https://www.lua.org/ftp/lua-5.4.7.tar.gz>.

The module replaces only `string.find`, `match`, `gmatch` and `gsub` in the skin
VM. `build.rs` compiles it against `DEP_LUA_INCLUDE` from the direct `mlua-sys`
dependency, which must stay compatible with the workspace's `mlua` dependency.
It uses Lua's public C API and its allocator; the linked Lua VM is unchanged.
The C optimization level matches `lua-src` even in development builds.

BMZ additions to review when updating the upstream code:

- `PatternWork` prepays blocks through a private mlua callback backed by the
  VM's shared atomic quotas. Errors therefore cannot discard uncharged work.
  The checker does not invoke skin code. Lua errors are raised through managed
  mlua callbacks and unwind within a protected Lua call; the C module owns no
  Rust values or resources requiring Rust destructors.
- Matching steps, bracket scans, balanced patterns, literal searches,
  backreferences and copied output bytes spend work. `find_byte` bounds each
  native `memchr` to 1024 bytes. Large comparisons are explicitly metered.
- A `gmatch` iterator keeps the quota checker as its fourth upvalue. Every
  resume gets new prepaid credit and restores recursion depth, including after
  a budget error in an earlier frame. It never resets the VM/frame counters.

Run the differential and resource tests with `cargo test -p bmz-skin --locked`.
Always initialize the pinned skin submodules and set `BMZ_REQUIRE_MZ_SELECT=1`
for compatibility validation; see `docs/development.md` at the repository root.
The synthetic CSV test also runs without those assets. The ignored
`benchmark_mz_select_csv` test compares stock and metered native operations on
the pinned real CSV; wall-clock timings are diagnostic, while work/instruction
ceilings are deterministic assertions in the synthetic regression test.
