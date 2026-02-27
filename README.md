<div align="center">
<h1>terminal-aichat</h1>
</div>


<div align="center"><img src="./aichat.webp" alt="terminal-aichat" height="140" /></div>

[README中文](./README_zh.md)

A CLI for AI/LLM chat in terminal
- written in rust, light (6.5MB binary size), super fast.
- multi platform(Windows, Linux, MacOS)
- using `/v1/chat/completion` API


```sh
aichat <INPUT MESSAGE>
```


## Quick Start

### Installation


#### sh
```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/slow-groovin/terminal-aichat/releases/latest/download/terminal-aichat-installer.sh | sh
```

#### cargo
```sh
cargo install terminal-aichat
```

#### homebrew
```sh
brew install slow-groovin/tap/terminal-aichat
```

#### ~~npm~~
>Deprecated, but you can still install version 0.3.5 with it

```sh
npm install terminal-aichat@latest
```

#### powershell
```sh
powershell -ExecutionPolicy Bypass -c "irm https://github.com/slow-groovin/terminal-aichat/releases/latest/download/terminal-aichat-installer.ps1 | iex"
```

#### binary
or download executable binaries directly in [Release](https://github.com/slow-groovin/terminal-aichat/releases) page.

### Prerequisites

Edit the config file to setup providers and models:

```sh
# First, get the config file location
aichat --list
# Or just:
aichat

# Then edit the config file (example for Linux)
nano ~/.config/terminal-aichat/config.jsonc
```

The config file contains a commented-out example that you can uncomment and modify.

### Chat

```sh
# Directly send a message
aichat how to view ubuntu release version

# If your message conflicts with an option, wrap it with quotes
aichat "how to use --config option"

# other ways
aichat "<INPUT MESSAGE>"
aichat -- <INPUT MESSAGE>

# pipe
cat input.txt | aichat
cat input.txt | aichat "explain this"

# pure mode (display for model/prompts configs and costs will be hide)
aichat --pure "Hello?"
```

## Configuration

### View Configurations

```sh
aichat --list
# or
aichat -l
```

This will show all configured providers, models, prompts, and the config file location.

### Edit Configuration

Configuration is done by directly editing the config file (JSONC format with comments support).

Config file locations (cross-platform):
- Linux: `~/.config/terminal-aichat/config.jsonc`
- macOS: `~/Library/Application Support/terminal-aichat/config.jsonc`
- Windows: `%APPDATA%\terminal-aichat\config.jsonc`

The config file uses JSONC format, which supports comments. A complete example is provided in the default config file.

Example config structure:
```jsonc
{
  "providers": {
    "openai": {
      "name": "OpenAI",
      "baseURL": "https://api.openai.com/v1",
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
      "baseURL": "https://openrouter.ai/api/v1",
      "apiKey": "sk-or-...",
      "models": {
        "meta-llama/llama-3-70b-instruct": {
          "name": "llama-3",
          "temperature": 0.3
        }
      }
    }
  },
  "prompts": {
    "sample_prompt": {
      "content": "You are a terminal assistant. You are giving help to user in the terminal. Give concise responses whenever possible. Because of terminal cannot render markdown, DO NOT contain any markdown syntax(`,```, #, ...) in your response, use plain text only.\n"
    }
  },
  "default-model": "gpt-5-mini",
  "default-prompt": "sample_prompt",
  "disable-stream": false,
  "pure": false,
  "verbose": false
}
```

### Specify a Model

```sh
# Use a specific model selector (searches all providers)
# - model key: "gpt-4o"
# - or provider/model_key: "openrouter/meta-llama/llama-3-70b-instruct"
aichat --model gpt-4o "Hello?"

# Or short form
aichat -m gpt-5-mini "Hello?"

# When model keys conflict across providers, or when model keys contain '/',
# use provider/model_key to disambiguate:
aichat -m openrouter/meta-llama/llama-3-70b-instruct "Hello?"
```

### Specify a Prompt

```sh
# Use a specific prompt
aichat --prompt concise "Hello?"

# Or short form
aichat -p sample_prompt "Hello?"
```

### Use Custom Config File

```sh
# Specify a custom config file path
aichat --config /path/to/config.jsonc --list
aichat --config /path/to/config.jsonc "Hello?"
```

### Use Temporary API Key via Environment Variable

> Useful for avoiding persistent API key storage or for testing.
> It will override API key in final request.

```sh
export OPENAI_API_KEY=sk-***************
aichat "Hello?"
```

### Set Log Level

```sh
export LOG_LEVEL=DEBUG
```

> Equivalent to using `--verbose`

### Pure Mode (`--pure`)

> Suppresses all extra messages and outputs only the response.

```sh
aichat --pure "Hello?"
```

### Verbose Logging (`--verbose`)

```sh
aichat --verbose "Hello?"
```

### Disable Streaming Mode (`--disable-stream`)

```sh
aichat --disable-stream "Hello?"
```
