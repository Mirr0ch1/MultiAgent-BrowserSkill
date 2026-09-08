# 防火墙放行规则（ufw / Gateway Host）

基于本机（Ubuntu + ufw）实测。网关机需要放行三个层面的流量，缺一不可——这是
us01 跨机排障学到的教训：**daemon 起来不等于外部能连，防火墙没放行一切白搭**。

## 1. 放行清单

| 端口/协议 | 用途 | 放行来源 |
|---|---|---|
| `52800/tcp` | 浏览器扩展 WebSocket（WS） | Tailscale 网段 + LAN 网段 |
| `52901/tcp` | 远程 agent CLI（TCP IPC） | Tailscale 网段 + LAN 网段 |
| `41641/udp` | **Tailscale 数据面**（WireGuard 隧道本身） | 任意（或按你的防火墙策略） |

## 2. 实际命令（ufw）

```bash
# 浏览器扩展 + 远程 agent（Tailscale CGNAT）
sudo ufw allow from 100.64.0.0/10 to any port 52800 proto tcp
sudo ufw allow from 100.64.0.0/10 to any port 52901 proto tcp

# 浏览器扩展 + 远程 agent（局域网，按你的网段替换）
sudo ufw allow from 192.168.10.0/24 to any port 52800 proto tcp
sudo ufw allow from 192.168.10.0/24 to any port 52901 proto tcp

# Tailscale 数据面（否则隧道无法建立/维持，跨机必然失败）
sudo ufw allow 41641/udp
```

> **注意**：如果只放行 52800/52901 而不放行 41641/udp，Tailscale 对端发来的包会在
> 数据面就被 ufw 丢弃（`Default: deny (incoming)` 时尤其如此），现象是「本机能连对方、
> 对方连不进来」——排障时先查这一条。

## 3. 验证放行生效

```bash
sudo ufw status numbered | grep -E "52800|52901|41641"
# 期望看到三条 ALLOW

# 从对端机器实测（代替盲猜）：
#   nc -vz <网关IP> 52901      # 远程 agent 端口
#   nc -vz <网关IP> 52800      # 扩展 WS 端口
```

## 4. 安全边界说明

- CIDR 白名单是**业务层**门控（daemon 内，握手前丢弃网段外 peer）；
  防火墙是**网络层**门控。两者叠加，纵深防御。
- 若希望「即使本机防火墙被关也有兜底」，务必保留 daemon 的 `--lan-cidr` 配置。
- `41641/udp` 放行「任意」是 Tailscale 官方推荐（它在 41641 上做 WireGuard 端点），
  信任边界由 Tailscale 自身（ACL/节点认证）承担。