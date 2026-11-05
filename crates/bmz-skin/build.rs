fn main() {
    println!("cargo:rerun-if-changed=native/lua_patterns.c");
    cc::Build::new()
        .file("native/lua_patterns.c")
        .include(std::env::var_os("DEP_LUA_INCLUDE").expect("mlua-sys Lua headers"))
        // Match lua-src's optimization even for development builds.
        .opt_level(2)
        .compile("bmz_lua_patterns");
}
