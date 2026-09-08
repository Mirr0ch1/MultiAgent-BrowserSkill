# 回滚指南（Rollback）：卸载 fork 回到上游 BrowserSkill

本 fork（MultiAgent-BrowserSkill）在保留上游 `upstream` remote 的前提下开发，回滚
成本刻意设计为「指回来 + 重装一次」即可。以下步骤全部实测可行。

## 1. 回滚场景判断

| 场景 | 做法 |
|---|---|
| 只是不用网关、退回单机原版体验 | 用上游 release 重装（见 §3），`BSK_REPO` 指回 Tencent/BrowserSkill |
| fork 有 bug 想回上游基线 | `git checkout upstream/main` 或删 fork clone 重 clone 上游 |
| 想彻底移除 fork | 卸载 `bsk`，删 `~/.bsk`，删扩展，见 §4 |

## 2. git 侧（保留上游引用）

```bash
cd <fork-clone>
git remote -v                       # 应看到 origin=fork, upstream=Tencent/BrowserSkill
# 回到与上游一致的基线（本 fork 所有改动都在 feature/gateway / main 上）
git fetch upstream
git checkout -b fallback-upstream upstream/main
# 或直接放弃本地改动回到上游 main：
git checkout main && git reset --hard upstream/main
```

> fork 的 main 已快进包含全部网关工作（26 commits on top of upstream）。回滚到
> `upstream/main` 即彻底回到腾讯原版代码。

## 3. 二进制侧（重装上游版）

上游 install 脚本支持 `BSK_REPO` 环境变量覆盖，fork 的 install.sh 默认值已改为
`Mirr0ch1/MultiAgent-BrowserSkill`——**显式指回上游即可**：

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/Tencent/BrowserSkill/main/install.sh | BSK_REPO=Tencent/BrowserSkill sh

# Windows (PowerShell)
$env:BSK_REPO="Tencent/BrowserSkill"
irm https://raw.githubusercontent.com/Tencent/BrowserSkill/main/install.ps1 | iex
```

验证：

```bash
bsk --version
bsk status          # 应显示上游版本号（如 0.2.x），且本地单机模式工作
```

## 4. 彻底移除

```bash
# 1) 停 systemd 服务（若本机部署过网关）
sudo systemctl stop bsk-gateway && sudo systemctl disable bsk-gateway
sudo rm /etc/systemd/system/bsk-gateway.service

# 2) 删除本地状态与二进制
rm -rf ~/.bsk
rm "$(which bsk)"           # 或删安装目录（默认 ~/.local/bin/bsk）

# 3) 浏览器扩展：Chrome 扩展页移除「MultiAgent-BrowserSkill」（load unpacked）

# 4) 防火墙规则（若按 ops/firewall-ufw.md 放过端口）
sudo ufw delete allow from 100.64.0.0/10 to any port 52800 proto tcp
sudo ufw delete allow from 100.64.0.0/10 to any port 52901 proto tcp
sudo ufw delete allow 41641/udp
```

## 5. 回滚后确认清单

- [ ] `bsk --version` 输出上游版本
- [ ] 上游扩展从 Chrome Web Store 重装（原版分发渠道），或 load unpacked 上游 build
- [ ] `bsk doctor` 全部 `ok`/`na`
- [ ] 防火墙里 fork 端口规则已删
- [ ] `~/.bsk` 已删（fork 的 daemon 状态不留残余）