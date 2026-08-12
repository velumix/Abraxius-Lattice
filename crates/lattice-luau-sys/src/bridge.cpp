#include "lattice-luau-sys/src/lib.rs.h"

#include "Luau/Allocator.h"
#include "Luau/Ast.h"
#include "Luau/Parser.h"

#include <string>

namespace lattice::luau {
namespace {

NativeSpan span(const Luau::Location& location)
{
    return NativeSpan{
        location.begin.line,
        location.begin.column,
        location.end.line,
        location.end.column,
    };
}

rust::String name(const Luau::AstName& value)
{
    return rust::String(value.value ? value.value : "");
}

std::string expressionPath(Luau::AstExpr* expression)
{
    if (auto global = expression->as<Luau::AstExprGlobal>())
        return global->name.value ? global->name.value : "";
    if (auto local = expression->as<Luau::AstExprLocal>())
        return local->local && local->local->name.value ? local->local->name.value : "";
    if (auto index = expression->as<Luau::AstExprIndexName>())
    {
        std::string base = expressionPath(index->expr);
        if (!base.empty() && index->index.value)
            return base + "." + index->index.value;
    }
    if (auto string = expression->as<Luau::AstExprConstantString>())
        return std::string(string->value.data, string->value.size);
    return "<dynamic>";
}

std::string functionName(Luau::AstExpr* expression)
{
    return expressionPath(expression);
}

class FactVisitor final : public Luau::AstVisitor
{
public:
    explicit FactVisitor(NativeAnalysis& output)
        : output(output)
    {
    }

    bool visit(Luau::AstStatLocal* node) override
    {
        for (Luau::AstLocal* local : node->vars)
        {
            if (local)
                output.symbols.push_back(NativeSymbol{name(local->name), 0, span(local->location)});
        }
        return true;
    }

    bool visit(Luau::AstStatLocalFunction* node) override
    {
        output.symbols.push_back(NativeSymbol{name(node->name->name), 1, span(node->location)});
        return true;
    }

    bool visit(Luau::AstStatFunction* node) override
    {
        output.symbols.push_back(NativeSymbol{rust::String(functionName(node->name)), 1, span(node->location)});
        return true;
    }

    bool visit(Luau::AstStatTypeAlias* node) override
    {
        output.symbols.push_back(NativeSymbol{name(node->name), 2, span(node->nameLocation)});
        return true;
    }

    bool visit(Luau::AstExprGlobal* node) override
    {
        output.references.push_back(NativeReference{name(node->name), 0, span(node->location)});
        return true;
    }

    bool visit(Luau::AstExprIndexName* node) override
    {
        output.references.push_back(NativeReference{name(node->index), 1, span(node->indexLocation)});
        return true;
    }

    bool visit(Luau::AstExprCall* node) override
    {
        std::string called = expressionPath(node->func);
        output.references.push_back(NativeReference{rust::String(called), 2, span(node->location)});

        if (called == "require" && node->args.size > 0)
        {
            output.require_facts.push_back(
                NativeRequire{rust::String(expressionPath(node->args.data[0])), span(node->args.data[0]->location)}
            );
        }
        return true;
    }

private:
    NativeAnalysis& output;
};

} // namespace

NativeAnalysis analyze(rust::Str source)
{
    NativeAnalysis output;
    Luau::Allocator allocator;
    Luau::AstNameTable names(allocator);
    Luau::ParseOptions options;
    std::string owned(source.data(), source.size());
    Luau::ParseResult result = Luau::Parser::parse(owned.data(), owned.size(), names, allocator, options);
    output.line_count = result.lines;

    for (const Luau::ParseError& error : result.errors)
    {
        output.diagnostics.push_back(
            NativeDiagnostic{rust::String(error.getMessage()), span(error.getLocation())}
        );
    }

    if (result.root)
    {
        FactVisitor visitor(output);
        result.root->visit(&visitor);
    }
    return output;
}

} // namespace lattice::luau
