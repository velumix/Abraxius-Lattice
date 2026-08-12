fn main() {
    let root = "../../native/luau";
    let mut build = cxx_build::bridge("src/lib.rs");
    build
        .std("c++20")
        .warnings(false)
        .include("include")
        .include(format!("{root}/Ast/include"))
        .include(format!("{root}/Common/include"));

    for source in [
        "Ast/src/Allocator.cpp",
        "Ast/src/Ast.cpp",
        "Ast/src/Confusables.cpp",
        "Ast/src/Cst.cpp",
        "Ast/src/Lexer.cpp",
        "Ast/src/Location.cpp",
        "Ast/src/Parser.cpp",
        "Ast/src/PrettyPrinter.cpp",
        "Common/src/BytecodeWire.cpp",
        "Common/src/StringUtils.cpp",
        "Common/src/TimeTrace.cpp",
    ] {
        build.file(format!("{root}/{source}"));
    }

    build.file("src/bridge.cpp").compile("lattice_luau_ast");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/bridge.cpp");
    println!("cargo:rerun-if-changed=include/lattice_luau_bridge.h");
    println!("cargo:rerun-if-changed={root}/Ast");
    println!("cargo:rerun-if-changed={root}/Common");
}
