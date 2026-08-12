//! Narrow, ownership-safe CXX boundary around the official Luau AST parser.

#![allow(unsafe_code)]

#[cxx::bridge(namespace = "lattice::luau")]
mod ffi {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct NativeSpan {
        pub begin_line: u32,
        pub begin_column: u32,
        pub end_line: u32,
        pub end_column: u32,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct NativeSymbol {
        pub name: String,
        pub kind: u8,
        pub span: NativeSpan,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct NativeReference {
        pub name: String,
        pub kind: u8,
        pub span: NativeSpan,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct NativeRequire {
        pub specifier: String,
        pub span: NativeSpan,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct NativeDiagnostic {
        pub message: String,
        pub span: NativeSpan,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct NativeAnalysis {
        pub symbols: Vec<NativeSymbol>,
        pub references: Vec<NativeReference>,
        pub require_facts: Vec<NativeRequire>,
        pub diagnostics: Vec<NativeDiagnostic>,
        pub line_count: u64,
    }

    unsafe extern "C++" {
        include!("lattice_luau_bridge.h");

        fn analyze(source: &str) -> NativeAnalysis;
    }
}

pub use ffi::{
    NativeAnalysis, NativeDiagnostic, NativeReference, NativeRequire, NativeSpan, NativeSymbol,
};

#[must_use]
pub fn analyze(source: &str) -> NativeAnalysis {
    ffi::analyze(source)
}
