# INIT.md — BrowserSkill Fork 网关化项目

> 项目启动与接力文档。接手本项目（任何 Agent）第一步：读本文件全文 + 评审文档，然后跑基线测试。
> 建立：2026-09-06 ｜ 维护：本文件随里程碑推进更新，改动同步写当日 memory。

## 1. 项目一句话

fork 腾讯 BrowserSkill，把 bsk daemon 改造成**局域网网关/broker（中转 + 状态注册机）**，实现 OpenClaw 多 agent（可多主机）× 多浏览器（Windows Chrome/Edge，日常已登录态）的「多对多」浏览器自动化。

## 2. 决策链（为什么走到 fork）

| 事件 | 结论 |
|---|---|
| OpenClaw 内置 browser 工具 | 已禁用（tools.deny 40 项），不可用 |
| 一级替代 | vercel/agent-browser（headless 自动浏览器）已部署，正常 |
| 二级 fallback | 腾讯 BrowserSkill（真实登录态/反爬/验证码场景），原版只支持单机（daemon 绑 127.0.0.1:52800 + CLI 走本机 UDS） |
| 旧方案被否 | 中心主机 SSH 到每台浏览器主机 → 中心持所有主机凭据，攻击面过大 |
| **新方案（本次）** | **fork 网关化：扩展主动出站连网关注册，agent 远程 CLI 直连 daemon** |
| 认证形态 | 共享 token 起步（操作简单 > 最大化安全），extension-id 白名单必须做，pairing 后置 |

## 3. 评审结论（两份评审共识）

- **可行性评级：高（有条件）**——CLI 与扩展共用 `bsk_protocol::Frame`，daemon 已具备注册表/select/队列等 broker 骨架，改造面集中在传输层 + 三处单机假设
- 投入封顶：**1.5~2 周当量（单人）**，超出即镀金
- 完整评审：`~/.openclaw/workspace-coder/projects/browserskill-gateway/FORK-REVIEW-GLM53.md`

### P0 缺口（不做不能上线）

1. **token 分角色**：agent_token（全权）+ extension_token（仅注册）；走首帧 handshake（禁 query string，防日志泄露）；`bsk token rotate` 轮换
2. **代码级互锁**：非 loopback 绑定且未配 token → daemon 拒绝启动（构造性安全）
3. **Busy 语义**：select 命中浏览器存在**其他 agent** 活跃 session → BUSY 错误（含占用者）；`--share` 显式覆盖；判定单位=「其他 agent 的活跃 session」非 session 计数
4. **session 归属隔离**：handshake 上报 agent_id，与 Busy **同一次协议改动**（bump 1.1→1.2）
5. **禁 daemon auto-update**（网关模式）——上游 Release 自动替换 = 配置/认证蒸发
6. **CLI auto-spawn 短路**（远程模式）——连不上网关直接报错，禁止静默拉起本地空 daemon
7. **防火墙交付物**：只放行 LAN/Tailscale 网段；优先绑 Tailscale 网卡 IP，不绑 0.0.0.0

### 三处未验证项（M1 第一天实测收编成测试）

- ① 扩展 ~20s 心跳是否阻止 daemon 30min idle 退出
- ② 同 instance_id 重连时，旧连接上活跃 session 的归宿（收养 vs 中断）
- ③ 扩展连局域网 IP 的 WS 是否受 Chrome Private Network Access 限制

## 4. 里程碑（修正后顺序）

- **M1 传输 + 认证 + 协议**（原 M2 核心前置）：WS 监听 LOCALHOST→可配置 listen；新增 TCP IPC listener + CLI `bsk --host <ip> --port <port>`；token 认证（WS + TCP 对称）+ 代码级互锁；agent_id + Busy 字段一次协议 bump 1.2；`--gateway` 聚合 flag（禁 idle/auto-spawn/auto-update，不可拆分）
- **M2 Busy/一致性集成测试**：并发语义单测 + 集成测试（协议在 M1 动完，此时跑）
- **M3 扩展 UI**：popup 可配 daemon 地址（IP 输入 → chrome.storage → 重连）+ 注册状态展示；可降级（首次手动配即可）
- **M4 skill 改造**（提前）：browser-login skill 改网关形态（`bsk --host` 直连、`bsk browsers` 列注册浏览器并询问用户）
- **M5 跨机 soak + 文档**：真跨机一轮 + 防火墙/systemd/回滚三件套文档

## 5. 关键技术事实（fork 基线 main @47ac947）

| 位置 | 事实 |
|---|---|
| `crates/bsk-cli/src/daemon/start.rs:240` | WS 地址硬编码 `Ipv4Addr::LOCALHOST` |
| `start.rs:70` | `daemon_idle: 60*30`（30min 空闲自杀） |
| `start.rs:239` | `spawn_update_check_task`（auto-update） |
| `start.rs:132-133` | CLI 连不上时 `spawn_detached` 本地拉起 daemon |
| `daemon/ws.rs:38` | 上游 TODO：extension-id allow-list（认证缺口自认） |
| `ws.rs:45/51` | origin 校验只查 `chrome-extension://` 前缀形状，LAN 可一秒伪造 → 安全全押 token |
| `ws.rs:214` | 强制首帧 `system.handshake`（token 挂点） |
| `daemon/browsers.rs:4` | 注册表按 instance_id 键控 |
| `browsers.rs:324` | `select()` 三级匹配：instance_id → label → 歧义报错 |
| `crates/bsk-protocol/src/system.rs` | HandshakeParams（token 字段加这里，serde default 向后兼容）；`compare_protocol`/`evaluate_handshake_compat`（bump 机制现成） |
| `apps/extension/src/entrypoints/background.ts:52` | 单 transport URL（`__BSK_DAEMON_WS_URL__` 构建期注入，wxt.config.ts:74 默认 ws://127.0.0.1:52800） |
| `apps/extension/manifest.json` | 无 `"key"` 字段 → unpacked 扩展 ID 每台随机（白名单前置问题） |
| `crates/bsk-cli/tests/` | **29 个集成测试**（ws_handshake/per_session_queue/idle_exit/auto_spawn/handshake_compat/ipc_smoke/cancel_forwarding…）→ 每次 rebase 的持续基线 |
| `install.sh:8-16` | `BSK_REPO`/`BSK_VERSION`/`BSK_INSTALL_DIR` 环境变量支持指向 fork Release |

## 6. 开发环境与命令

- 本机：Ubuntu 26.04（THUNDEROBOT 游戏本），IP 192.168.10.99，Tailscale 可用，sshd active
- fork 仓库：`~/coding/browserskill`（origin=https://github.com/Tencent/BrowserSkill.git，main @47ac947）
- Rust：rust-toolchain.toml = stable + rustfmt + clippy；rustup 安装于 `~/.cargo`
- 扩展：apps/extension（wxt + vitest），构建 `pnpm build`
- Windows 侧（米罗配合，M5）：PowerShell `irm https://raw.githubusercontent.com/Tencent/BrowserSkill/main/install.ps1 | iex` + 手动装扩展 + 提供 IP/用户名

## 7. 验收清单（简版，全版见评审 §5.3）

- [ ] 上游 29 个集成测试在 fork 仓库全绿（基线存档）
- [ ] 无 token 的 WS/TCP 连接首帧后即拒 + 落日志
- [ ] 非 loopback + 无 token → daemon 拒绝启动
- [ ] 双浏览器注册 + 歧义 label 走 CLI 提示
- [ ] agent B select 被占浏览器 → BUSY 含占用者；改选另一台并行成功
- [ ] 断线重连重注册 ≤60s，注册表无重名残留
- [ ] kill daemon → systemd 5s 拉起 → 扩展自动重连；挂机 1h idle 不退出
- [ ] 远程模式连不上 → 报错不 auto-spawn
- [ ] 卸载 fork → BSK_REPO 指回上游，功能与改造前一致

## 8. 进度追踪

- [x] 调研（两级替代可行 + 原版单机约束确认）
- [x] 部署：deny 内置 browser；agent-browser 0.36.0 + Chrome 152 装入；两个 skill 改名装入 `~/.openclaw/skills`（browser-auto / browser-login）
- [x] fork clone 到 `~/coding/browserskill`
- [x] 双份评审（本 agent + GLM-5.3）+ 复核（证据全部验证属实）
- [x] INIT.md 建立
- [x] Rust 工具链就绪（rustup 1.98.1 stable，~/.cargo/env）
- [x] 基线：`cargo test -p bsk -- --test-threads=1` **274 全绿**存档（分支 feature/gateway；并行首跑 1 个失败为 env 竞争 flaky，串行通过）
- [x] **M1 完成（9 个 commit，全部测试绿）**：协议 1.2（token/agent_id）+ WS 可配 listen + TCP IPC（首帧 agent-token 认证）+ 互锁 + 禁 auto-update + WS extension_token + TcpClient/remote 短路 + Busy 语义/session 归属 + --share override
- [x] **M2 完成（e1fac0d，12 个新集成测试全绿）**：测试入口 daemon::run 支持 TCP IPC + token（TcpIpcHandle/bind_server/DaemonHandle.tcp）；gateway_tcp_auth（3）/ gateway_ws_token（4）/ gateway_busy（2）/ gateway_runtime_guards（3）——TCP 认证、WS 扩展 token、跨 agent Busy、互锁、远程 auto-spawn 短路全覆盖
- [x] **M3 完成（5c1ffcb，扩展侧 15 文件）**：popup 可配 daemon 地址 + 扩展 token——daemon-config 存储层、握手协议 1.2 + HandshakeParams.token、WSTransport.reconfigure 运行时换地址、ConnectionController.replaceTransport/setExtensionToken、background 启动读配置 + popup set_daemon_url/set_extension_token 消息、popup connection 设置视图 + i18n（en/zh）；扩展测试 846 全绿
- [x] **M4 完成（skill 网关形态，码仓外）**：`~/.openclaw/skills/browser-login/SKILL.md` 与 `browser-auto/SKILL.md` 同步——browser-login 移除 Bash(ssh:*) 与已否的 SSH 接入段，改写为网关直连形态（`bsk --host <ip> --port <port> --agent-token <token>` 全局 flag + BSK_AGENT_TOKEN 环境变量；远程模式不 auto-spawn；先 `bsk browsers` 列注册浏览器、多台询问用户；带 `--agent-id` 保 Busy 语义）；browser-auto 路由引用同步网关描述
- [ ] M5 跨机 soak + 防火墙/systemd/回滚文档 + Windows 真机
- [ ] M2 Busy/一致性测试
- [ ] M3 扩展 UI
- [ ] M4 skill 网关形态
- [ ] M5 跨机 soak + 文档 + Windows 真机

## 9. 相关文件索引

- 本文件：`~/coding/browserskill/INIT.md`
- GLM-5.3 评审：`~/.openclaw/workspace-coder/projects/browserskill-gateway/FORK-REVIEW-GLM53.md`
- 当日决策记忆：`~/.openclaw/workspace-coder/memory/2026-09-06.md`
- 配置备份：`~/.openclaw/openclaw.json.bak.20260906_*`

---

## 10. M1 精确改动点清单（2026-09-06 开工，源码读完钉死）

### 已核实的关键实现事实（读码所得，非推断）

- `DaemonConfig`（start.rs）现有字段：ws_port / session_idle / daemon_idle / allow_any_origin / extension_connect_wait / browser_liveness_timeout / browser_liveness_tick；`From<&StartArgs>` 构建
- `StartArgs`（cli/daemon.rs）：port / foreground / session_idle / daemon_idle；`resolved_*()` 方法
- WS 绑定：`start.rs:240` `SocketAddr::new(Ipv4Addr::LOCALHOST, cfg.ws_port)` 硬编码
- IPC：UDS listener（Unix）/ NamedPipe（Windows），`ipc::bind` + `serve(listener, handler, …)`；`IpcServer::bind(sock_path)` 生成 `IpcHandle{sock_path, shutdown, task}`
- WS 首帧强制 `system.handshake`（ws.rs drive_connection），`HANDSHAKE_FIRST_FRAME_TIMEOUT=5s`；handshake 拒绝即断连
- `HandshakeParams`（bsk-protocol/system.rs:434）：client / version / protocol_version / instance_id / browser / label / min_compatible_peer(弃用) / min_compatible_protocol(Option, serde default) —— **新增字段加这里，serde default 向后兼容**
- `PROTOCOL_VERSION="1.1"`（state.rs:20）、`MIN_COMPATIBLE_PROTOCOL="1.0"`（state.rs:22）
- 协议兼容机制现成：`evaluate_handshake_compat`（system.rs:42）+ `compare_protocol`（system.rs:111），popup version_skew UI 已存在
- `RequestFrame{id, method, params}`（frame.rs），CLI↔daemon 与 daemon↔扩展**同构**（评审核心证据之一）
- `select()`（browsers.rs:324）：instance_id → label → AmbiguousLabel，**无占用检查**（Busy 挂点）
- `ensure_daemon()`（cli/ensure_daemon.rs）：读 daemon.json → 无则 `spawn_daemon()`（auto-spawn 点）
- `ipc_client.rs`：Unix `Client::connect` → `connect_path(sock_path)`；Windows NamedPipeClient（ERROR_PIPE_BUSY 重试 5s）
- `DaemonState`（state.rs）：config / browsers / sessions / tool_queues / abort_registry / tool_inflight / session_interrupts / transfers

### 改动清单（按依赖序）

**① 协议层（bsk-protocol）**
- [ ] `system.rs` HandshakeParams 新增 `token: Option<String>` + `agent_id: Option<String>`（均 serde default + skip_serializing_if，旧客户端零影响）
- [ ] `state.rs` `PROTOCOL_VERSION` "1.1"→"1.2"（MIN_COMPATIBLE_PROTOCOL 保持 1.0，旧扩展 minor skew 仍兼容，version_skew UI 提示）
- [ ] `system.rs` HandshakeResult 同步加 server 返回字段告知所需 token 角色（可选）

**② CLI 参数层（bsk-cli）**
- [ ] `cli/daemon.rs` StartArgs 新增：`--listen <IP>`（WS 绑定地址，默认 127.0.0.1）、`--agent-port <PORT>`（TCP IPC 端口，默认 None=不启用）、`--gateway`（聚合 flag）、以及 token 配置来源（`--token` 或配置文件 `~/.bsk/gateway.json`）
- [ ] `cli/mod.rs` GlobalFlags 或 Command 级新增 `--host <IP>` / `--port <PORT>`（远程 CLI 连接，评审设计稿）
- [ ] `cli/daemon.rs` `resolved_*()` 相应新增

**③ Daemon 配置层（start.rs）**
- [ ] `DaemonConfig` 新增字段：listen_ip / agent_port / gateway_mode / agent_token / extension_token
- [ ] `From<&StartArgs>` 映射
- [ ] `new(port)` 测试构造器同步（默认真实值）

**④ WS 绑定 + 互锁（start.rs）**
- [ ] `run_foreground` WS 地址用 `cfg.listen_ip` 替代 LOCALHOST
- [ ] 互锁：`listen_ip` 非 loopback 且 token 未配置 → 拒绝启动并输出明确错误

**⑤ TCP IPC listener（ipc.rs + start.rs）**
- [ ] `ipc.rs` 新增 TcpListener 版本 bind/serve（复用 handler/line 协议，仿 UDS 路径）
- [ ] `run_foreground` 按 `cfg.agent_port` 绑定 TCP listener + 注册到 DaemonState/IPC 分派
- [ ] TCP IPC 首帧强制 handshake + token（与 WS 侧对称——评审 P0：全功能控制面裸露网络，认证强度 ≥ WS）

**⑥ token 认证（ws.rs + ipc.rs）**
- [ ] `ws.rs` drive_connection 在 `system.handshake` 处理处校验 token：extension_token 仅允许注册类；agent_token 全权（Busy/session 管理）
- [ ] 校验失败 → 断连 + warn 日志（挂 ws.rs:214 现有首帧处理点）
- [ ] TCP IPC 侧对称实现

**⑦ CLI 远程连接（ipc_client.rs + ensure_daemon.rs）**
- [ ] `ipc_client.rs` Client 新增 `connect_tcp(host, port)`；`ensure_daemon()` 远程模式短路 auto-spawn（连不上直接报错）
- [ ] `cli/mod.rs` 各业务命令走统一 resolve（本地 UDS / 远程 TCP）

**⑧ 网关模式聚合行为（start.rs）**
- [ ] `--gateway`：① 默认绑 Tailscale/LAN 网卡 IP（探测 tailscale0 优先）② daemon_idle=None ③ auto-spawn 短路 ④ auto-update 禁用（不 spawn_update_check_task）⑤ session_idle 保持默认 5min

**⑨ Busy 语义 + session 归属（browsers.rs + sessions.rs + ipc.rs）**
- [ ] session 记录 agent_id（CLI handshake 上报，`HandshakeParams.agent_id`）
- [ ] `select()` 命中浏览器存在其他 agent 活跃 session → SelectError::Busy{agent_id}；同 agent 多 session 放行；`--share` 显式覆盖
- [ ] `bsk browsers` 输出带 agent 占用信息

### M1 验证策略（每步可独立验证）
- 协议层改完 → `cargo test -p bsk` 全绿（含 ws_handshake 兼容断言更新）
- 参数层改完 → `bsk daemon start --help` 冒烟 + 单测
- 每完成一层跑一次全量，最后跑跨 TCP 的集成测试

### 未验证项（M1 第一天实测，收编测试）
1. 扩展 ~20s 心跳是否阻止 daemon idle 退出（gateway 模式禁 idle 前先实测确认现状）
2. 同 instance_id 重连时旧 session 归宿（收养 vs 中断）
3. 扩展连局域网 IP WS 是否受 Chrome PNA 限制