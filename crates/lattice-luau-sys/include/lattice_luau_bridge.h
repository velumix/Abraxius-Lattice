#pragma once

#include "rust/cxx.h"

namespace lattice::luau {

struct NativeAnalysis;
NativeAnalysis analyze(rust::Str source);

} // namespace lattice::luau

