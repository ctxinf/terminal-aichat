
<div align="center">
<h1>terminal-aichat</h1>
</div>


<div align="center"><img src="./aichat.webp" alt="terminal-aichat" height="140" /></div>
<div align="center"><img src="./aichat-terminal-integration.webp" alt="terminal-aichat-shell-integration"  height="210" /></div>

终端内AI/LLM聊天的CLI
- 使用Rust编写，轻量级（6.5MB二进制大小），超级快。
- 跨平台（Windows, Linux, MacOS）
- 使用 `/v1/chat/completion` API

```sh
aichat <INPUT MESSAGE>
```

## 快速入门

### 安装


#### sh
```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/ctxinf/terminal-aichat/releases/latest/download/terminal-aichat-installer.sh | sh
```

#### cargo
```sh
cargo install terminal-aichat
```

#### homebrew
```sh
brew install ctxinf/tap/terminal-aichat
```
#### ~~npm~~
>Deprecated, but you can still install version 0.3.5 with it

```sh
npm install terminal-aichat@latest
```
#### powershell
```sh
powershell -ExecutionPolicy Bypass -c "irm https://github.com/ctxinf/terminal-aichat/releases/latest/download/terminal-aichat-installer.ps1 | iex"
```
#### binary
在[Release](https://github.com/ctxinf/terminal-aichat/releases)页面中直接下载二进制程序。

### 前置要求
编辑配置文件来设置provider和models：

```sh
# 首先获取配置文件位置
aichat config show
# 或者直接运行：
aichat

# 然后编辑配置文件（以Linux为例）
nano ~/.config/terminal-aichat/config.jsonc
```

配置文件中包含一个注释掉的示例，您可以取消注释并修改。

### chat
```sh
# 直接发送消息
aichat how to view ubuntu release version

# 如果消息与选项冲突，用引号包裹
aichat "how to use --config option"

# 其他方式
aichat "<INPUT MESSAGE>"
aichat -- <INPUT MESSAGE>

# 管道
cat input.txt | aichat
cat input.txt | aichat "explain this"

# 纯净模式（不显示模型/提示配置和成本信息）
aichat --pure "Hello?"
```

### Shell 集成 (`?` / `?!`)

一行装好，给你的 shell 加两个命令：

```sh
# zsh
eval "$(aichat --init-integration zsh)"
# bash
eval "$(aichat --init-integration bash)"
# fish
aichat --init-integration fish | source
```

| 命令 | 行为 |
| --- | --- |
| `? <问题>` | 问 AI；如果回的是可执行命令，**预填到当前命令行**（你按 Enter 执行），否则显示正文，并在终端加上淡青色 `aichat` 标签。 |
| `?! <问题>` | 同上，但**自动执行**解析出的命令。每行先以 `# …`（暗色）显示，让你看清要跑什么。 |
| `cmd \| ? "..."` | 通过管道把 stdin 作为额外上下文。 |
| `? --debug ...` | 额外把模型原始回复作为注释显示（仅 `?`）。 |

集成用的提示词会写入配置 `prompts.shell-exec-or-chat`——**可读、可改、可换**（用 `--prompt <name>` 切换）。bash/fish 标准名是 `q` / `qe`（`?` / `?!` 是尽力别名）。

生成的脚本会保留选定的 `--config` 路径与 `--prompt`，提供 shell/OS 上下文，并固定使用纯净、非流式回复。日志输出到 stderr。命令解析需要 `jq` 或 `python3`；两者都没有时仅显示回复，不执行。重定向聊天输出时只保留正文。升级后请重新加载 integration。

## 配置

### 查看配置

```sh
aichat config show
# 或者
aichat config show
```

这将显示所有已配置的providers、models、prompts以及配置文件位置。

### 编辑配置

通过直接编辑配置文件进行配置（支持注释的JSONC格式）。

配置文件位置（跨平台）：
- Linux: `~/.config/terminal-aichat/config.jsonc`
- macOS: `~/Library/Application Support/terminal-aichat/config.jsonc`
- Windows: `%APPDATA%\terminal-aichat\config.jsonc`

配置文件使用JSONC格式，支持注释。默认配置文件中提供了完整示例。

配置结构示例：
```jsonc
{
  "defaultProvider": "openai",
  "defaultModel": "gpt-5-mini",
  "defaultPrompt": "concise",
  "defaultThinkingLevel": "low",
  "providers": {
    "openai": {
      "baseUrl": "https://api.openai.com/v1",
      "apiKey": { "value": "your-key" },
      "models": [
        { "id": "gpt-5-mini", "stream": true },
        {
          "id": "custom-model",
          "name": "My model",
          "reasoning": false,
          "extraBody": { "temperature": 0.7 },
          "removeBodyFields": ["store"]
        }
      ]
    },
    "local": {
      "baseUrl": "http://127.0.0.1:11434/v1",
      "models": [{ "id": "local-model" }]
    }
  },
  "prompts": {
    "concise": { "system": "Use plain text, give concise output." }
  }
}
```

### 指定模型

```sh
# 使用特定模型 selector（会搜索所有 providers）
# - model ID: "gpt-4o"
# - 或 provider/model-id: "openrouter/meta-llama/llama-3-70b-instruct"
aichat --model gpt-4o "Hello?"

# 或者短格式
aichat -m gpt-5-mini "Hello?"

# 当多个 provider 里存在相同的 model ID，或者 model ID 本身包含 '/' 时，
# 使用 provider/model-id 来避免歧义：
aichat -m openrouter/meta-llama/llama-3-70b-instruct "Hello?"
```

### 指定提示词

```sh
# 使用特定提示词
aichat --prompt concise "Hello?"

# 或者短格式
aichat -p sample_prompt "Hello?"
```

提示词可拆分为 `system` 和 `user`，两者都支持以下模板：

```jsonc
"prompts": {
  "context": {
    "system": "你是终端助手。目录：{{ cwd }}\n系统：{{ shell(\"cat /etc/os-release\") }}",
    "user": "请回答：{{ input }}"
  }
}
```

支持 `{{ input }}`、`{{ cwd }}`、`{{ env("LANG") }}` 和
`{{ shell("command -v git") }}`。省略 `user` 时直接发送原始输入。
函数接受一个 JSON 字符串参数；模板内的双引号在 JSON 配置中需要转义。
`shell()` 将 stdout 插入提示词，支持管道、重定向和 `$()`，例如
`{{ shell("printf '%s' \"$(command -v git)\"") }}`。
仅支持这些显式表达式，不支持 Jinja 的循环、条件和过滤器。

命令仅在发送请求前执行，每次请求渲染一次，网络重试复用结果；
查看配置和帮助不会执行。插入的用户输入、环境变量和命令输出不会再次解析。
命令在当前工作目录执行，Unix 使用 `/bin/sh -c`，Windows 使用 `cmd /D /S /C`。
超时为 3 秒，stdout 与 stderr 合计上限 64 KiB；非零退出、超时、
输出超限或 stdout 不是 UTF-8 都会中止请求。stdout 仅去掉末尾换行，
stderr 仅用于失败诊断。Unix 超时或失败时会终止整个命令进程组。
执行前使用代码写死的黑名单，拦截常见变更命令、解释器和写文件重定向。
检查较保守，作为参数出现的黑名单词也可能被拒绝；Git 仅允许路径查询和 `--version`。
黑名单不是 shell 沙箱，配置中仍应只使用你信任的命令。

旧的 `content` 仍作为原样 system 消息兼容，不能与 `system` 同时配置。
通常在 `system` 放上下文，在 `user` 使用 `input`，避免重复发送问题。

### 使用自定义配置文件

```sh
# 指定自定义配置文件路径
aichat config --config /path/to/config.jsonc show
aichat --config /path/to/config.jsonc "Hello?"
```

### 模型与请求配置

`models` 改为数组，`id` 是请求使用的模型 ID，`name` 仅用于显示。
`defaultProvider` 限定不带 provider 的 ID/名称搜索；`provider/model-id`
可以显式切换 provider。未指定默认 provider 时全局搜索，重名时报错，ID 优先于名称。

`reasoning` 默认 true；`thinkingLevel` 覆盖 `defaultThinkingLevel`（默认 low），
以原始字符串发送为 `reasoning_effort`。设为 false 时显式请求关闭思考。
请求覆盖顺序：基础请求 → provider.extraBody → model.extraBody → removeBodyFields。
浅合并，嵌套值整体替换；`temperature` 等 API 参数放入 `extraBody`。

provider 支持 `api`（仅 openai-completions，默认值）、`headers`、`queryParams`、
`timeoutMs`（默认 120000）、`maxRetries`（默认 2）、`extraBody`。
model.headers 不区分大小写地覆盖 provider.headers，自定义头覆盖自动认证头。
header 值支持 `$NAME` / `${NAME}` 插值及显式 `!command`；仅在选中模型发起请求时
执行命令，10 秒超时，进程内缓存成功结果。命令配置仅用于可信文件。

### API 凭据

凭据直接配置在 provider 内：`"apiKey": {"value": "your-key"}`。无认证服务省略 apiKey。

```sh
aichat config init
aichat config login --provider openrouter
```

login 隐藏输入并要求再次输入确认，使用 AES-256-GCM + Argon2id 加密后保存为
`{"value":"v1:...","encryption":"generated-passphrase"}`，保留 JSONC 注释。
密文和加密口令同存配置，文件可读者仍可解密；Unix 下 login 将文件权限设为 0600。
删除顶层 auth 和 key 匹配机制，value 不再展开环境变量。

使用 `aichat config show`、`aichat config provider list`、`aichat config model list` 查看配置，凭据不会打印。
子命令参数独立，例如 `aichat config --config ./custom.jsonc show`；聊天参数和管道输入保持原有行为。
要发送字面值 config，可用 `aichat -- config`。

#### 设置日志级别
```sh
export TERMINAL_AICHT_LOG_LEVEL=DEBUG
```

#### 使用纯净模式 (`--pure`)
> 纯净模式不显示任何提示信息

```sh
aichat --pure "Hello?"
```
#### 显示详细日志 (`--log-level`)
```sh
aichat --log-level DEBUG "Hello?"
```

DEBUG 显示模板字段、shell 命令及失败详情（退出状态和捕获的输出）；TRACE 还会显示渲染后的消息和成功命令的 stdout。原 `--verbose` / `TERMINAL_AICHT_VERBOSE` / `verbose` 配置项请改用 `--log-level` / `TERMINAL_AICHT_LOG_LEVEL` / `logLevel`。
#### 不使用stream方式调用api (`--disable-stream`)
```sh
aichat --disable-stream "Hello?"
```


### Display and agent documentation

`aichat --skill` prints the detailed [agent usage guide](src/cli/skill.txt), including configuration, environment variables and shell integration.

```sh
aichat --display=usage,model,prompt,reasoning,duration "Hello"
TERMINAL_AICHT_DISPLAY=usage,duration aichat "Hello"
```

Display defaults to empty. Config example: `"display": {"usage": true, "model": true, "duration": true}`.
Environment variables with the `TERMINAL_AICHT_` prefix override CLI values, which override configuration values.
Use `--display=` to clear display options; `--pure` suppresses them all.
See `--skill` for the complete argument-to-environment mapping.

Streaming is configured per model: `"stream": true` (default) or `false`. The old top-level `disableStream` field is removed; `--disable-stream` and `TERMINAL_AICHT_DISABLE_STREAM` override the model for one invocation.
