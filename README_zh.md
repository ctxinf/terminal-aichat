
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
aichat --list
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
| `? <问题>` | 问 AI；如果回的是可执行命令，**预填到当前命令行**（你按 Enter 执行），否则用 `###` 包裹原样打印。 |
| `?! <问题>` | 同上，但**自动执行**解析出的命令。每行先以 `# …`（暗色）显示，让你看清要跑什么。 |
| `cmd \| ? "..."` | 通过管道把 stdin 作为额外上下文。 |
| `? --debug ...` | 额外把模型原始回复作为注释显示（仅 `?`）。 |

集成用的提示词会写入配置 `prompts.shell-exec-or-chat`——**可读、可改、可换**（用 `--prompt <name>` 切换）。bash/fish 标准名是 `q` / `qe`（`?` / `?!` 是尽力别名）。

## 配置

### 查看配置

```sh
aichat --list
# 或者
aichat -l
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
      "apiKey": "$MY_OPENAI_API_KEY",
      "models": [
        { "id": "gpt-5-mini" },
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
  "auth": {
    // Optional fallback when MY_OPENAI_API_KEY is unset.
    // Encrypted values use a matching generated encryption passphrase.
    // "openai-account": { "key": "MY_OPENAI_API_KEY", "value": "your-key" }
  },
  "prompts": {
    "concise": { "content": "Use plain text, give concise output." }
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

### 使用自定义配置文件

```sh
# 指定自定义配置文件路径
aichat --config /path/to/config.jsonc --list
aichat --config /path/to/config.jsonc "Hello?"
```

### 模型与请求配置

`models` 改为数组，`id` 是请求使用的模型 ID，`name` 仅用于显示。
`defaultProvider` 限定不带 provider 的 ID/名称搜索；`provider/model-id`
可以显式切换 provider。未指定默认 provider 时全局搜索，重名时报错，ID 优先于名称。

`reasoning` 默认 true；`thinkingLevel` 覆盖 `defaultThinkingLevel`（默认 low），
以原始字符串发送为 `reasoning_effort`。设为 false 时不生成此字段。
请求覆盖顺序：基础请求 → provider.extraBody → model.extraBody → removeBodyFields。
浅合并，嵌套值整体替换；`temperature` 等 API 参数放入 `extraBody`。

provider 支持 `api`（仅 openai-completions，默认值）、`headers`、`queryParams`、
`timeoutMs`（默认 120000）、`maxRetries`（默认 2）、`extraBody`。
model.headers 不区分大小写地覆盖 provider.headers，自定义头覆盖自动认证头。
header 值支持 `$NAME` / `${NAME}` 插值及显式 `!command`；仅在选中模型发起请求时
执行命令，10 秒超时，进程内缓存成功结果。命令配置仅用于可信文件。

### API 凭据

provider.apiKey 支持字面值或完整的 `$NAME` / `${NAME}` 引用。先读取对应环境变量，
未设置时查找顶层 auth 中 key 等于 NAME 的条目。条目仅允许 key、value、encryption
三个可选字符串字段；value 默认按字面使用，设置 encryption 后按固定 v1 格式
解密（AES-256-GCM + Argon2id）。value 和 encryption 不插值、不执行命令。
环境凭据为空或无效、解密失败时直接报错。不隐式读取 OPENAI_API_KEY，不加载环境文件。
本地无认证服务可省略 apiKey，也可使用 `"auth": {"type": "none"}`。

```sh
export MY_OPENAI_API_KEY=your-key
aichat "Hello?"
```

#### 设置日志级别
```sh
export LOG_LEVEL=DEBUG
```
> 目前, 等同于 `--verbose`

#### 使用纯净模式 (`--pure`)
> 纯净模式不显示任何提示信息

```sh
aichat --pure "Hello?"
```
#### 显示详细日志 (`--verbose`)
```sh
aichat --verbose "Hello?"
```
#### 不使用stream方式调用api (`--disable-stream`)
```sh
aichat --disable-stream "Hello?"
```
