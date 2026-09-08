# systemd 托管网关 daemon（Gateway Host 运维指南）

本文档基于本机（Ubuntu 26.04）实测：`bsk-gateway.service` 由 systemd 托管，daemon 以
`--gateway` 模式常驻，`Restart=always` 保证 5 秒内自愈拉起。

## 1. 单元文件

`/etc/systemd/system/bsk-gateway.service`：

```ini
[Unit]
Description=MultiAgent-BrowserSkill gateway daemon (bsk, gateway mode)
After=network-online.target tailscaled.service
Wants=network-online.target

[Service]
Type=simple
User=mirro
WorkingDirectory=/home/mirro/coding/browserskill
ExecStart=/home/mirro/coding/browserskill/target/release/bsk daemon start \
  --gateway \
  --listen 0.0.0.0 \
  --agent-port 52901 \
  --agent-token <AGENT_TOKEN> \
  --extension-token <EXTENSION_TOKEN> \
  --lan-cidr 100.64.0.0/10 \
  --lan-cidr 192.168.10.0/24 \
  --lan-cidr 192.168.30.0/24 \
  --lan-cidr 192.168.50.0/24 \
  --foreground
Restart=always
RestartSec=5
Environment=BSK_AGENT_TOKEN=<AGENT_TOKEN>
Environment=BSK_EXTENSION_TOKEN=<EXTENSION_TOKEN>
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

要点：
- **`--foreground` 必须加**：systemd `Type=simple` 需要前台进程；不加会 double-fork 导致
  systemd 认为服务已退出。
- **`--listen 0.0.0.0`**：同时监听 loopback 与所有网卡（Tailscale/LAN），本机扩展默认连
  `ws://127.0.0.1:52800` 也能命中；跨机 peer 由 `--lan-cidr` + token 双重门控。
- **`--agent-port 52901`**：TCP IPC 端口，远程 agent CLI 用 `--host <ip> --port 52901` 直连。
- **token 双角色**：`--agent-token`（远程 CLI 全权）与 `--extension-token`（浏览器扩展仅注册），
  互锁要求非 loopback 绑定必须配齐。
- **CIDR 白名单**：只允许 100.64.0.0/10（Tailscale CGNAT）+ 三个 192.168 网段的 peer 进入；
  即使 token 泄露，网段外来源在握手前被丢。**生产环境请替换成你自己的网段，不要照抄**。
- **token 明文落盘**：单元文件内嵌 token（`ps` 可见）。LAN 内共享信任域可接受（设计决策：
  操作简单 > 最大化安全）；如需更强，改用 `EnvironmentFile=/etc/bsk-gateway.env`（chmod 600）。

## 2. 安装与启停

```bash
sudo cp bsk-gateway.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now bsk-gateway.service   # 开机自启 + 立即起
sudo systemctl restart bsk-gateway.service        # 改完单元后重载
sudo systemctl status bsk-gateway.service
```

## 3. 日志

```bash
sudo journalctl -u bsk-gateway.service -f          # 跟随
sudo journalctl -u bsk-gateway.service -n 200      # 最近 200 行
# bsk 自身日志也落 ~/.bsk/daemon.log.YYYY-MM-DD
```

## 4. 自愈验证（soak 标准）

```bash
sudo systemctl kill -s SIGKILL bsk-gateway.service  # 模拟 daemon 崩溃
sleep 7
systemctl is-active bsk-gateway.service             # 应为 active（RestartSec=5）
# 浏览器扩展应在 ~30s 内自动重连并重新注册（心跳 + 重连 supervisor）
```

## 5. 防火墙联动

见 `firewall-ufw.md`。要点：`52800/tcp`（WS）、`52901/tcp`（TCP IPC）、`41641/udp`
（Tailscale 数据面）必须对可信网段放行，否则跨机连不上——这是 us01 排障学到的教训。