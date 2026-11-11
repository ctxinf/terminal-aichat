# terminal-aichat bash integration
#
# Provides:
#   q  <message>    ask AI; if reply is a command, pre-fill an editable prompt
#   qe <message>    ask AI; if a command is parsed, echo it as `# …`
#                   comments then execute it; if parsing fails, dump the
#                   reply as chat text (no execution)
#   command | q     pipe stdin as additional context
#   q --debug ...   prepend the raw model reply as a comment line (`q` only)
#
# Bash does not allow `?` as a function name. We expose `q` / `qe` as the
# canonical names, and additionally register `?` / `?!` as aliases (modern
# bash accepts them; we silently skip if your build rejects them).
#
# Install:
#   eval "$(aichat --init-integration bash)"
#
# To use a custom prompt name (will be added to your config under `prompts`):
#   eval "$(aichat --init-integration bash --prompt <name>)"

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
  local debug=0
  local -a args=()
  local a
  for a in "$@"; do
    if [ "$a" = "--debug" ]; then debug=1; else args+=("$a"); fi
  done
  if [ "${#args[@]}" -eq 0 ] && [ -t 0 ]; then
    echo "usage: q <message>   (or:  cmd | q [message])" >&2
    return 1
  fi

  local output cmd
  output=$(_aichat_run "${args[@]}") || return $?
  cmd=$(printf '%s' "$output" | _aichat_extract_cmd)

  if [ -n "$cmd" ]; then
    if [ ! -t 1 ]; then
      printf '%s\n' "$cmd"
      return 0
    fi
    if [ "$debug" = 1 ]; then
      local comment
      comment=$(printf '%s' "$output" | sed 's/^/# /' | tr '\n' ' ')
      printf '%s\n' "$comment"
    fi
    # bash has no `print -z`; use `read -e -i` to put the command in
    # an editable prompt. User edits then presses Enter to run, or Ctrl+C to abort.
    local edited
    if IFS= read -e -i "$cmd" -r -p '' edited < /dev/tty; then
      if [ -n "$edited" ]; then
        history -s -- "$edited"
        eval "$edited"
      fi
    fi
  else
    _aichat_show_chat "$output"
  fi
}

_aichat_ask_exec() {
  if [ "$#" -eq 0 ] && [ -t 0 ]; then
    echo "usage: qe <message>   (or:  cmd | qe [message])" >&2
    return 1
  fi

  local output cmd
  output=$(_aichat_run "$@") || return $?
  cmd=$(printf '%s' "$output" | _aichat_extract_cmd)

  if [ -n "$cmd" ]; then
    # Parse succeeded: echo the resolved command as `# …` comments before
    # executing (multi-line aware). Running model-generated commands is risky;
    # this gives the user a last chance to spot something wrong.
    # When stdout is a tty, color the comment lines like a real shell comment
    # (bright-black / dim grey, matching most syntax-highlighting themes).
    if [ -t 1 ]; then
      local _dim=$'\e[90m' _rst=$'\e[0m'
      printf '%s\n' "$cmd" | sed "s/^/${_dim}# /;s/\$/${_rst}/"
    else
      printf '%s\n' "$cmd" | sed 's/^/# /'
    fi
    history -s -- "$cmd"
    eval "$cmd"
  else
    # Plain text or an invalid command reply is displayed without execution.
    _aichat_show_chat "$output"
  fi
}

alias q='_aichat_ask'
alias qe='_aichat_ask_exec'
# Attempt to also bind `?` / `?!`. Bash treats `?` as a pattern character,
# but many bash builds accept it as an alias name. Silence the error if not.
alias '?'='_aichat_ask' 2>/dev/null || true
alias '?!'='_aichat_ask_exec' 2>/dev/null || true
