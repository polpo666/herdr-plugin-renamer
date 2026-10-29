# LM 命名引擎使用教程（中文）

fork 版 herdr-plugin-renamer（github:polpo666/herdr-plugin-renamer）内置了
**原生 LAN LM 命名引擎**：一次 OpenAI-compatible 的 chat-completion 请求，把
agent 的第一条 prompt 变成 kebab-case 短语，自动命名 pane、worktree 分支和
workspace。纯 Rust 实现，不依赖 Python、codex CLI 或任何包装脚本。

## 1. 它帮你做什么

在 herdr 里开一个 worktree 并启动 agent 后（Claude Code / Codex / Pi 均可）：

```
worktree/brave-harbor-74e0  →  zfmai/add-e2e-ok-readme   （分支，自动加前缀）
workspace label "lme2e2"    →  add-e2e-ok-readme         （workspace 名）
pane title                   →  add-e2e-ok-readme         （侧栏/tab 名）
```

全程无感：agent 开始工作后几秒内完成，失败自动回退，绝不阻塞 herdr。

## 2. 安装（新机器一条命令）

```sh
herdr plugin install polpo666/herdr-plugin-renamer --yes
```

要求：herdr ≥ 0.7.4、Rust 工具链（`cargo`）。Linux/macOS 均可（Linux 上跳过
macOS 专属的 on-device helper 构建）。

## 3. 配置 LM 服务器

```sh
CFG=$(herdr plugin config-dir herdr-plugin-renamer)   # 默认 ~/.config/herdr/plugins/config/herdr-plugin-renamer
cat > "$CFG/lm.json" <<'EOF'
{
  "url": "https://lm.example.net/v1/chat/completions",
  "resolve": "lm.example.net:443:192.168.50.141",
  "model": "qwen3.8-flash-next-nvfp4",
  "timeout_seconds": 22
}
EOF
echo zfmai > "$CFG/branch-prefix"    # 可选：分支前缀
```

字段说明：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `url` | ✅ | OpenAI-compatible 完整 URL，一般以 `/v1/chat/completions` 结尾 |
| `resolve` | ➖ | DNS 直连钉扎，`host:port:ip` 格式（同 curl `--resolve`）。流量直发该 IP，但 TLS SNI 和证书校验仍按域名走。服务器有公网域名+内网 IP 时用它 |
| `model` | ✅ | 模型名 |
| `timeout_seconds` | ➖ | 整个请求超时，默认 22；连接阶段固定 3 秒上限 |

**`lm.json` 存在即启用 LM 引擎**——它自动排在引擎链最前面，无需任何环境变量。

环境变量覆盖（可选）：`HERDR_NAMING_LM_URL`、`HERDR_NAMING_LM_MODEL`；
分支前缀也可用 `HERDR_NAMING_BRANCH_PREFIX` 覆盖文件。

## 4. 安装后验证

```sh
ROOT=$(herdr plugin list --json | python3 -c \
  "import json,sys; print([p['plugin_root'] for p in json.load(sys.stdin)['result']['plugins'] if p['plugin_id']=='herdr-plugin-renamer'][0])")
HERDR_PLUGIN_CONFIG_DIR=$(herdr plugin config-dir herdr-plugin-renamer) \
  "$ROOT/target/release/herdr-plugin-renamer" --lm-test "给登录接口加重试"
```

打印一个 slug（如 `add-login-retry`）即成功；任何配置/网络错误会打错误并退非 0。

## 5. 端到端冒烟测试（可选，约 1 分钟）

```sh
# 任意 git 仓库开一个后台 worktree，取 root_pane.pane_id
herdr worktree create --cwd ~/some-repo --label smoketest --no-focus
herdr agent start smoketest --kind pi --pane <pane_id>
herdr agent prompt smoketest "在 README 末尾加一行 smoke ok" --wait --timeout 120000
herdr workspace list | grep -A2 smoketest      # label 应变成 LM 生成的 slug
cd <worktree path> && git branch --show-current  # 应变成 zfmai/<slug>

# 清理
herdr agent kill smoketest
herdr worktree remove --workspace <workspace_id> --force
```

## 6. 引擎链与选择

默认链（`lm.json` 存在时）：

- macOS：**LM → Apple on-device FoundationModels → Codex** → 本地确定性 slug
- Linux：**LM → Codex** → 本地确定性 slug

任何一环失败自动落下一环，全部失败用 prompt 本地生成的保底 slug——所以
LM 服务器挂了也不影响工作，只是命名变丑。

固定某个引擎：`export HERDR_NAMING_ENGINE=lm|foundation|codex`（herdr 启动
环境里生效）。不设则用默认链。

## 7. 更新与排障

```sh
herdr plugin update herdr-plugin-renamer --yes       # 更新
herdr plugin log list --plugin herdr-plugin-renamer --limit 10   # 运行日志
```

常见问题：

- **slug 是 prompt 的原文片段** → LM 请求失败走了保底，检查 `lm.json`、
  LM 服务是否可达；用 `--lm-test` 复现。
- **中文 prompt** → 没问题，系统提示词要求模型输出英文 slug（已实测）。
- **没重命名** → 看事件是否触发：agent 必须从 idle → working 过渡；分支只在
  还是自动名 `worktree/*` 时才改，手工建过的分支不会被碰。
- **DNS 钉扎** → `resolve` 只对 `url` 里的 host 生效，格式必须是
  `host:port:ip` 三段。

## 8. 当前部署状态

| 机器 | 状态 |
| --- | --- |
| 本机 | `github:polpo666/herdr-plugin-renamer@bbc49e5`，e2e 实测通过 |
| maimac (maicc) | 同版本，`--lm-test` 通过（192.168.50.201 → LM 192.168.50.141） |

旧架构（local.lm-renamer 包装器 + disabled 官方插件）已于 2026-10-29 全部退役。
