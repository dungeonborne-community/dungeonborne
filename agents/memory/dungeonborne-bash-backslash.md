---
name: dungeonborne-bash-backslash
description: "12.11: in this environment the Bash tool collapses \\\\ into \\ even inside heredoc <<'EOF' — edits with \\r\\n, Windows paths etc. break"
metadata:
  visibility: team
  node_type: memory
  type: feedback
---

In the Bash tool a double backslash turns into a single one even in `cat <<'EOF'` / `python - <<'EOF'`: `"\\r\\n"` in a Python script became a real CR/LF inside Rust strings, `Kit\\run` in FIX_LIST became `Kit` + CR + `un`.

**Why:** on 12.11 this broke strings in launcher profile.rs (Rust compiled, but with line breaks inside the strings) and a path in docs/FIX_LIST.md; they had to be fixed separately.
**How to apply:** do edits with backslashes via Edit/Write, or via a script written with Write into the scratchpad and run by path; in Python code use `chr(92)` instead of `\\`. After the edit, check the bytes (`python -c ... find`). Related to [[dungeonborne-workflow]].
