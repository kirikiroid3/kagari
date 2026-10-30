# Summary

[Introduction](./introduction.md)

# Foundations

- [Design Decisions](./foundations/decisions.md)
- [Relation to the Proposal](./foundations/proposal.md)
- [Repository Layout](./foundations/layout.md)

# Compiler

- [Pipeline and Queries](./compiler/pipeline.md)
- [Syntax: Lexer, CST, AST and Forms](./compiler/syntax.md)
- [Name Resolution and HIR](./compiler/resolve.md)
- [Type Inference and Checks](./compiler/typeck.md)
- [Diagnostics](./compiler/diagnostics.md)
- [THIR and MIR](./compiler/mir.md)
- [Constant Evaluation](./compiler/eval.md)
- [Story and System Analysis](./compiler/analysis.md)
- [Emitting Bytecode](./compiler/emit.md)
- [IDE Layer](./compiler/ide.md)

# Standard Library

- [Builtins and the Host ABI](./std/builtins.md)
- [The std–Checker Contract](./std/contract.md)

# Formats

- [The `.kagari` Format](./formats/kagari.md)
- [Archives and Save Databases](./formats/archive.md)
- [`.kagari-cache` Files](./formats/cache.md)

# Runtime

- [The VM](./runtime/vm.md)
- [The Engine](./runtime/engine.md)
- [Saves, Traces and Snapshots](./runtime/saves.md)
- [Text and Rendering](./runtime/text-render.md)
- [Shaders](./runtime/shaders.md)
- [The Desktop Runtime](./runtime/desktop.md)

# Tooling

- [Projects and the CLI](./tooling/cli.md)
- [Formatter](./tooling/fmt.md)
- [Language Server and Editors](./tooling/lsp.md)

# Process

- [Testing](./process/testing.md)
- [Documentation](./process/docs.md)
- [Open Questions](./process/open-questions.md)
