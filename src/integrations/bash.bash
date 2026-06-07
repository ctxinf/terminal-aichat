# terminal-aichat bash integration
#
# Provides:
#   q  <message>    ask AI; if reply is a command, pre-fill an editable prompt
#   qe <message>    ask AI; if a command is parsed, echo it as `# …`
#                   comments then execute it; if parsing fails, dump the
#                   full reply between `###` markers (no execution)
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
    jq -r '.command? // empty' 2>/dev/null
  elif command -v python3 >/dev/null 2>&1; then
    python3 -c $'import sys,json\ntry:\n  d=json.load(sys.stdin); v=d.get("command");\n  print(v if isinstance(v,str) else "")\nexcept Exception: pass' 2>/dev/null
  else
    sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1
  fi
}

_aichat_run() {
  command aichat --prompt {{PROMPT_NAME}} --pure --disable-stream "$@"
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
  output=$(_aichat_run "${args[@]}")
  cmd=$(printf '%s' "$output" | _aichat_extract_cmd)

  if [ -n "$cmd" ]; then
    if [ "$debug" = 1 ]; then
      local comment
      comment=$(printf '%s' "$output" | sed 's/^/# /' | tr '\n' ' ')
      printf '%s\n' "$comment"
    fi
    # bash has no `print -z`; use `read -e -i` to put the command in
    # an editable prompt. User edits then presses Enter to run, or Ctrl+C to abort.
    local edited
    if IFS= read -e -i "$cmd" -r -p '' edited; then
      if [ -n "$edited" ]; then
        history -s -- "$edited"
        eval "$edited"
      fi
    fi
  else
    printf '###############################################\n'
    printf '%s\n' "$output"
    printf '###############################################\n'
  fi
}

_aichat_ask_exec() {
  if [ "$#" -eq 0 ] && [ -t 0 ]; then
    echo "usage: qe <message>   (or:  cmd | qe [message])" >&2
    return 1
  fi

  local output cmd
  output=$(_aichat_run "$@")
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
    # Parse failed: dump the full model reply between markers.
    printf '###############################################\n'
    printf '%s\n' "$output"
    printf '###############################################\n'
  fi
}

alias q='_aichat_ask'
alias qe='_aichat_ask_exec'
# Attempt to also bind `?` / `?!`. Bash treats `?` as a pattern character,
# but many bash builds accept it as an alias name. Silence the error if not.
alias '?'='_aichat_ask' 2>/dev/null || true
alias '?!'='_aichat_ask_exec' 2>/dev/null || true
