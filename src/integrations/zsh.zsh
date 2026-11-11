# terminal-aichat zsh integration
#
# Provides:
#   ? <message>     ask AI; if reply is a command, put it into the buffer
#   ?! <message>    ask AI; if a command is parsed, echo it as `# …`
#                   comments then execute it; if parsing fails, dump the
#                   reply as chat text (no execution)
#   command | ?     pipe stdin as additional context
#   ? --debug ...   prepend the raw model reply as a comment line (`?` only)
#
# Install:
#   eval "$(aichat --init-integration zsh)"
#
# To use a custom prompt name (will be added to your config under `prompts`):
#   eval "$(aichat --init-integration zsh --prompt <name>)"

# Extract .command from JSON reply. Tries jq, then python3, then a sed fallback.
_aichat_extract_cmd() {
  if command -v jq >/dev/null 2>&1; then
    jq -ers 'if length == 1 and (.[0] | type == "object") then .[0].command | select(type == "string") | select(test("[^[:space:]]") and (explode | index(0) == null)) else empty end' 2>/dev/null
  elif command -v python3 >/dev/null 2>&1; then
    python3 -c 'import sys,json
try:
  d=json.load(sys.stdin)
  v=d.get("command") if isinstance(d,dict) else None
  if not isinstance(v,str) or not v.strip() or chr(0) in v: sys.exit(1)
  sys.stdout.write(v)
except Exception: sys.exit(1)' 2>/dev/null
  else
    printf '%s\n' 'aichat: install jq or python3 to enable command replies.' >&2
    return 1
  fi
}

# Run aichat with the shell-exec-or-chat prompt and collect output.
# The prompt itself lives under prompts in your selected config file.
# so you can read and edit it — run `aichat config show` to see the location.
_aichat_run() {
  env -u TERMINAL_AICHT_INIT_INTEGRATION \
    TERMINAL_AICHT_PROMPT={{PROMPT_NAME}} \
    TERMINAL_AICHT_CONFIG={{CONFIG_PATH}} \
    TERMINAL_AICHT_PURE=true TERMINAL_AICHT_DISABLE_STREAM=true \
    aichat --integration-shell {{SHELL_NAME}} -- "$@"
}

_aichat_show_chat() {
  if [ -t 1 ]; then
    printf '\033[36maichat\033[0m\n'
  fi
  printf '%s\n' "$1"
}

_aichat_ask() {
  emulate -L zsh
  local debug=0
  local -a args
  args=()
  local arg
  for arg in "$@"; do
    if [[ "$arg" == "--debug" ]]; then
      debug=1
    else
      args+=("$arg")
    fi
  done
  if (( ${#args[@]} == 0 )) && [[ -t 0 ]]; then
    print -u2 "usage: ? <message>   (or:  cmd | ? [message])"
    return 1
  fi

  local output cmd
  output=$(_aichat_run "${args[@]}") || return $?
  cmd=$(printf '%s' "$output" | _aichat_extract_cmd)

  if [[ -n "$cmd" ]]; then
    if [ ! -t 1 ]; then
      printf '%s\n' "$cmd"
      return 0
    fi
    if (( debug )); then
      local comment
      comment=$(printf '%s' "$output" | sed 's/^/# /' | tr '\n' ' ')
      print -z "${comment}
$cmd"
    else
      print -z -- "$cmd"
    fi
  else
    _aichat_show_chat "$output"
  fi
}

_aichat_ask_exec() {
  emulate -L zsh
  if (( $# == 0 )) && [[ -t 0 ]]; then
    print -u2 "usage: ?! <message>   (or:  cmd | ?! [message])"
    return 1
  fi

  local output cmd
  output=$(_aichat_run "$@") || return $?
  cmd=$(printf '%s' "$output" | _aichat_extract_cmd)

  if [[ -n "$cmd" ]]; then
    # Parse succeeded: echo the resolved command as `# …` comments before
    # executing (multi-line aware). Running model-generated commands is risky;
    # this gives the user a last chance to spot something wrong.
    # When stdout is a tty, color the comment lines like a real shell comment
    # (bright-black / dim grey, matching most syntax-highlighting themes).
    if [[ -t 1 ]]; then
      local _dim=$'\e[90m' _rst=$'\e[0m'
      printf '%s\n' "$cmd" | sed "s/^/${_dim}# /;s/\$/${_rst}/"
    else
      printf '%s\n' "$cmd" | sed 's/^/# /'
    fi
    print -s -- "$cmd"
    eval "$cmd"
  else
    # Plain text or an invalid command reply is displayed without execution.
    _aichat_show_chat "$output"
  fi
}

# Use `noglob` so `?` and `*` in user input aren't expanded by zsh globbing.
alias '?'='noglob _aichat_ask'
alias '?!'='noglob _aichat_ask_exec'
