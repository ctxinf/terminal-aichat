# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**terminal-aichat** is a CLI tool for AI/LLM chat in terminal:
- Written in Rust (2024 edition), lightweight (~6.5MB binary), very fast
- Multi-platform: Windows, Linux, macOS
- Uses OpenAI-compatible `/v1/chat/completion` API
- Extremely simple and easy to use

## Common Commands

### Build
```bash
cargo build                    # Development build
cargo build --release          # Release build (optimized)
cargo build --profile dist     # Dist profile (optimized with thin LTO)
```

### Test
```bash
cargo test                     # Run all tests
cargo test <test_name>         # Run specific test
cargo test --verbose           # Run tests with verbose output
```

### Lint
```bash
cargo clippy                   # Run linter
cargo clippy --fix --bin "aichat"  # Auto-fix suggestions
```

### Run
```bash
cargo run -- [args]            # Run development binary
cargo run -- --help            # Show help
```

## Code Architecture

### Directory Structure
```
src/
├── main.rs                    # Entry point
├── chat.rs                    # API communication with OpenAI
├── cli/                       # CLI parsing & command handling
│   ├── cli.rs                 # Main command handler
│   ├── structs.rs             # clap argument definitions
│   ├── interactive.rs         # Interactive input mode
│   └── response_render.rs     # Response rendering with typewriter effect
├── config/                    # Configuration management
│   ├── structs.rs             # Serialization/deserialization models
│   ├── manager.rs             # File I/O operations
│   ├── builder.rs             # ConfigBuilder for fluent API
│   ├── resolver.rs            # Config merging logic
│   └── display.rs             # Pretty-printing for config listing
└── utils/                     # Utility functions
    ├── logger.rs              # Logging system
    └── string.rs              # String extensions and masking
```

### Key Architectural Patterns

1. **Configuration System**:
   - `ConfigBuilder` pattern for fluent config creation
   - Config resolution order: CLI args > file config > defaults
   - Cross-platform storage via `dirs` crate

2. **Command Handling**:
   - clap with derive attributes for declarative CLI
   - Subcommands: `set`, `use`, `delete`, `list`
   - Pipe input detection and interactive mode support

3. **API Communication**:
   - `async-openai` crate for OpenAI-compatible endpoints
   - Streaming and non-streaming response handling
   - API key override via `OPENAI_API_KEY` env var

4. **Response Rendering**:
   - `ResponseRenderer` with typewriter effect (30 chars/second)
   - Status bar with model/prompt info
   - Pure mode for clean output

### Configuration File Locations
- Linux: `~/.config/terminal-aichat/config.json`
- macOS: `~/Library/Application Support/terminal-aichat/config.json`
- Windows: `%APPDATA%\terminal-aichat\config.json`

### Important Dependencies
- **async-openai**: OpenAI API client
- **clap**: CLI framework with derive
- **crossterm**: Terminal handling
- **tokio**: Async runtime (full features)
- **serde_json**: JSON configuration
- **dirs**: Cross-platform config directories

### Example Configuration JSON

```json
{
  "models": {
    "sample_model_gpt": {
      "model_name": "gpt-5-mini",
      "base_url": "https://api.openai.com/v1",
      "api_key": null,
      "temperature": null
    }
  },
  "prompts": {
    "sample_prompt": {
      "content": "You are a terminal assistant. \nYou are giving help to user in the terminal.\nGive concise responses whenever possible.\nBecause of terminal cannot render markdown, DO NOT contain any markdown syntax(`,```, #, ...) in your response, use plain text only.\n"
    }
  },
  "default-model": "sample_model_gpt",
  "default-prompt": "sample_prompt",
  "disable-stream": false,
  "pure": false,
  "verbose": false
}
```

A more complete example with multiple models:

```json
{
  "models": {
    "openai_gpt4": {
      "model_name": "gpt-4o",
      "base_url": "https://api.openai.com/v1",
      "api_key": "sk-...",
      "temperature": 0.7
    },
    "openrouter": {
      "model_name": "openai/gpt-oss-20b:free",
      "base_url": "https://openrouter.ai/api/v1",
      "api_key": "sk-or-...",
      "temperature": 0.3
    }
  },
  "prompts": {
    "sample_prompt": {
      "content": "You are a terminal assistant. You are giving help to user in the terminal. Give concise responses whenever possible. Because of terminal cannot render markdown, DO NOT contain any markdown syntax(`,```, #, ...) in your response, use plain text only.\n"
    },
    "concise": {
      "content": "Use plain text, give extremely concise output"
    }
  },
  "default-model": "openai_gpt4",
  "default-prompt": "sample_prompt",
  "disable-stream": false,
  "pure": false,
  "verbose": false
}
```

---

## 彻底改革计划 (Radical Refactor Plan)

### 目标 (Goals)
简化配置方式，面向开发者，移除复杂的 CLI 配置命令，改为直接编辑配置文件。

### 原因 (Why)
- 本工具面向开发者，不需要复杂的 CLI 来进行配置
- 直接编辑配置文件更简单、更灵活
- 减少代码维护成本

### TODO 列表

1. **CLI 结构改革**
   - [x] 删除 `set`、`use`、`delete` 子命令, 保留 `list` 子命令（后续实现: ）,保留 `--model`/`-m` 参数(后续实现)，
   - [x] 启用 `--config` 参数，允许手动指定配置文件路径
   - [x] 在 `-h`/`--help` 输出中提示默认配置文件位置
   - [x] 更新 CLI 的 about 信息

2. **配置文件解析改革**
   - [x] 添加 `jsonc` 依赖库
   - [x] 修改配置读取逻辑：先尝试读取 `.json`，再尝试读取 `.jsonc`
   - [x] 使用 jsonc 库解析配置文件

3. **Model 配置结构完全更改**
   - [x] 重新设计 `Config` 结构体
   - [x] 新结构：`providers: { "<provider_key>": { name?: string, baseUrl: string, apiKey: ..., models: { "<model_key>": { name?: string, temperature: ... } } } }`
   - [x] provider 的 name 可以为空，为空时以 key 作为 name
   - [x] model 的 name 可以为空，为空时以 key 作为 name
   - [x] 更新默认配置生成逻辑
   - [x] 保持 `prompts` 配置结构不变
   - [x] 保持其他参数（`disable-stream`、`pure`、`verbose`）不变

4. **配置管理模块重构**
   - [x] 删除 `builder.rs`（不再需要 ConfigBuilder）
   - [x] 删除 `resolver.rs`（简化配置合并逻辑）
   - [x] 修改 `display.rs` 以适配新的 list 命令显示逻辑（provider-name: - model-names）
   - [x] 重构 `manager.rs` 以适应新的配置结构
   - [x] 更新 `structs.rs` 定义新的配置结构

5. **Chat 模块适配**
   - [x] 更新 `chat.rs` 以适配新的 provider/model 配置结构
   - [x] 更新 API 调用逻辑

6. **CLI 处理模块更新**
   - [x] 重构 `cli/cli.rs` 删除 `set`/`use`/`delete` 子命令处理逻辑
   - [x] 修改 `list` 子命令实现，显示新的配置结构, 显示简要配置列表：provider-name: - model-names
   - [x] 更新 `cli/structs.rs` 移除 `set`/`use`/`delete` 子命令定义
   - [x] 实现 `--model`/`-m` 参数的新逻辑：遍历 providers  的所有模型，直到命中一个
   - [x] 保持 `interactive.rs` 和 `response_render.rs` 不变

7. **更新示例配置**
   - [ ] 更新 CLAUDE.md 中的示例配置为新格式

### 新的配置结构示例 (New Config Structure Example)

```jsonc
{
  "providers": {
    "openai": {
      "name": "OpenAI",
      "baseUrl": "https://api.openai.com/v1",
      "apiKey": "sk-...",
      "models": {
        "gpt-4o": {
          "name": "gpt-4o",
          "temperature": 0.7
        },
        "gpt-5-mini": {
          "name": "gpt-5-mini",
          "temperature": 0.5
        }
      }
    },
    "openrouter": {
      "name": "OpenRouter",
      "baseUrl": "https://openrouter.ai/api/v1",
      "apiKey": "sk-or-...",
      "models": {
        "llama-3": {
          "name": "meta-llama/llama-3-70b-instruct",
          "temperature": 0.3
        }
      }
    },
    "local": {
      "name": null,
      "baseUrl": "http://localhost:11434/v1",
      "apiKey": null,
      "models": {
        "llama3.1": {
          "name": null,
          "temperature": 0.8
        }
      }
    }
  },
  "prompts": {
    "sample_prompt": {
      "content": "You are a terminal assistant. You are giving help to user in the terminal. Give concise responses whenever possible. Because of terminal cannot render markdown, DO NOT contain any markdown syntax(`,```, #, ...) in your response, use plain text only.\n"
    },
    "concise": {
      "content": "Use plain text, give extremely concise output"
    }
  },
  "default-provider": "openai",
  "default-model": "gpt-5-mini",
  "default-prompt": "sample_prompt",
  "disable-stream": false,
  "pure": false,
  "verbose": false
}
```

注：
- provider 的 `name` 为 `null` 或省略时，使用 key（如 `"local"`）作为 name
- model 的 `name` 为 `null` 或省略时，使用 key（如 `"llama3.1"`）作为 name

### 实施计划 (Implementation Plan)

**阶段 1：配置结构重构**
1. 更新 `Cargo.toml` 添加 jsonc 依赖
2. 重写 `config/structs.rs` 定义新结构（provider + models 二级结构，name 可选）
3. 更新默认配置逻辑

**阶段 2：CLI 调整**
1. 修改 `cli/structs.rs` 移除 `set`/`use`/`delete` 子命令，保留 `list` 子命令
2. 启用 `--config` 参数
3. 更新 help 信息，提示默认配置文件位置
4. 保留 `--model`/`-m` 参数定义

**阶段 3：配置管理重构**
1. 删除 `builder.rs`、`resolver.rs`
2. 修改 `display.rs` 适配新的 list 命令显示逻辑（provider-name: - model-names）
3. 重写 `manager.rs`
4. 更新配置读取逻辑：先尝试 .json，再尝试 .jsonc

**阶段 4：CLI 处理逻辑更新**
1. 重构 `cli/cli.rs` 删除 `set`/`use`/`delete` 处理逻辑
2. 实现新的 `list` 命令显示逻辑
3. 实现 `--model`/`-m` 参数新逻辑：遍历 providers 所有模型直到命中

**阶段 5：Chat 模块适配**
1. 更新 `chat.rs` 使用新配置结构

**阶段 6：清理和测试**
1. 删除无用代码
2. 运行测试
3. 更新 CLAUDE.md 示例配置
