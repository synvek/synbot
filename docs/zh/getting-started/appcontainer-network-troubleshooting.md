# AppContainer 沙箱：出站网络不通

若守护进程在启用网络的 Windows AppContainer 沙箱内运行，但出站 HTTPS 失败（例如飞书/OpenAI 报 "error sending request for url"），可按以下步骤排查是哪个 Windows 筛选平台 (WFP) 筛选器拦截了连接。

## 首次运行（或重启后）：用管理员运行一次

为 AppContainer 添加防火墙出站规则和 WFP 放行需要**管理员**权限。规则是**持久化**的：沙箱停止时**不会**被删除。因此：

- **安装后或重启后首次运行**：以**管理员身份**运行一次 `synbot sandbox`（或实际用于启动 AppContainer 守护进程的命令），例如右键 → 以管理员身份运行。程序会添加防火墙与 WFP 规则，沙箱停止后规则仍会保留。
- **之后的运行**：任意用户（包括普通用户）均可启动沙箱；已有规则会放行 AppContainer 的出站网络，无需管理员。
- **重启之后**：Windows 会清除 WFP 筛选器，防火墙规则可能仍保留。以管理员身份再运行一次以重新添加 WFP（及可能缺失的防火墙规则），之后普通用户即可再次运行。若规则曾被手动或策略删除，同样以管理员运行一次即可重新添加。

## 关闭 WFP 丢包审计（恢复）

排查完成后，建议**关闭** WFP 丢包审计以降低安全日志量。以**管理员**身份运行 PowerShell 或 CMD：

```powershell
auditpol /set /subcategory:"{0CCE9225-69AE-11D9-BED3-505054503030}" /success:disable /failure:disable
```

确认已关闭：执行 `auditpol /get /subcategory:"{0CCE9225-69AE-11D9-BED3-505054503030}"`，应显示“成功”和“失败”均为“已禁用”。

## 根因区分：DNS (11001) 与 WFP

若守护进程日志出现 **raw_os_error=Some(11001)**（WSAHOST_NOT_FOUND）或 **dns error: proto error: no connections available**，说明是 AppContainer 内的 **DNS** 问题：系统解析器在此不可用，依赖系统 DNS 的解析器拿不到 nameserver（无法读取系统 DNS 配置）。Synbot 在 reqwest 中使用 **hickory-dns**，在 AppContainer 内启动诊断时使用自定义 **GoogleDnsResolver**（通过 `ResolverConfig::google()` 使用 Google 8.8.8.8），因此诊断请求不依赖系统配置。若看到 `AppContainer network diagnostic: GET https://www.microsoft.com -> 200`，说明使用显式 DNS 的出站 HTTPS 是通的。**open-lark（飞书）** 客户端仍使用默认解析器；若出现 "no connections available"，可确认容器内系统 nameserver 列表为空。DNS 失败不会产生 5152/5157 事件。

## 确认能力与 SID

沙箱日志中应能看到：

- `CreateAppContainerProfile with 4 or 5 capability SIDs`（启用网络时包含 INTERNET_CLIENT 和 PRIVATE_NETWORK_CLIENT_SERVER）
- `SECURITY_CAPABILITIES: CapabilityCount=4`（或 5）
- `AppContainerSid=S-1-15-2-...`（父进程与子进程一致）

若其中任一项缺失或为 0，请先修正 token/能力。若均正确，则拦截发生在 WFP。

## 查找拦截连接的 WFP 筛选器

1. **启用 WFP 丢包审计**（以管理员身份运行 PowerShell 或 CMD）：

   使用子类别 **GUID**（在非英文 Windows 上可避免“参数错误”，例如显示名被本地化时）：

   ```powershell
   auditpol /set /subcategory:"{0CCE9225-69AE-11D9-BED3-505054503030}" /success:enable /failure:enable
   ```

   若失败，可列出子类别以获取本机名称（例如中文 Windows 上的“筛选平台数据包丢弃”）：

   ```powershell
   auditpol /list /subcategory:*
   ```

   然后用引号内的准确名称，例如：`auditpol /set /subcategory:"筛选平台数据包丢弃" /success:enable /failure:enable`。

2. **复现故障**：在沙箱中启动 synbot 并触发飞书（或任意出站 HTTPS）。

3. **打开事件查看器** → Windows 日志 → **安全**。按**事件 ID 5152** 和 **5157** 筛选，时间范围选你运行 synbot 的时段。查找：
   - **任务类别**：筛选平台数据包丢弃  
   - **方向**：出站  
   - **源/目标**：本机 → open.feishu.cn（或目标 IP）

4. 在事件详情中记下 **筛选器运行时 ID** 和 **层运行时 ID**。筛选器运行时 ID 即拦截该数据包的筛选器。

5. **与 WFP 状态对应**：
   - 导出状态：`netsh wfp show state file=wfpstate.xml`（需管理员）。
   - 打开 `wfpstate.xml`，搜索 `<filterId>RUN_TIME_ID</filterId>`（将 RUN_TIME_ID 替换为事件中的值）。其附近的 `<displayData><name>...</name>` 为筛选器名称；`<layerKey>` 为层。

6. **解读**：
   - 若拦截筛选器为 **SynBot AppContainer Outbound V4/V6**，说明我们的放行筛选器存在但未匹配（例如条件或 SID 不一致）。
   - 若拦截筛选器来自 **MPSSVC** / **App Isolation** 等，则为系统规则拒绝自定义 AppContainer；可能需要调整我们的筛选器权重/子层或检查 OS/安全基线。

7. **完成后关闭审计**（可选，以减少日志量）：

   ```powershell
   auditpol /set /subcategory:"{0CCE9225-69AE-11D9-BED3-505054503030}" /success:disable /failure:disable
   ```

## 若安全日志中没有 5152/5157（无 WFP 丢包事件）

若已启用审计但看不到 AppContainer 进程的**任何**出站拦截事件，失败可能发生在 **WFP 之前**（例如 Winsock/token）或不会记录 5152 的路径。

1. **查看守护进程日志**中在 AppContainer 内启动时执行的那条一次性诊断：
   - `AppContainer network diagnostic: GET https://www.microsoft.com -> 200` → 启动时出站正常（失败可能发生在之后或仅与飞书相关）。
   - `AppContainer network diagnostic: request failed io_error kind=... raw_os_error=...` → 关注 **raw_os_error**：
     - **11001** (WSAHOST_NOT_FOUND) = AppContainer 内 DNS 解析失败（使用带 **hickory-dns** 的 reqwest；见上文“根因：DNS (11001)”）。
     - **10013** (WSAEACCES) = 访问被拒绝（常见于 token 或 WFP 前的防火墙，或未记录审计的 WFP 拦截）。
     - **10061** (WSAECONNREFUSED) = 服务器拒绝连接。
     - **10060** (WSAETIMEDOUT) = 超时。
   - `AppContainer network diagnostic: request timed out after 5s` → 5 秒内无响应（可能是拦截或网络/DNS 问题）。

2. **确认审计已开启**：运行 `auditpol /get /subcategory:"{0CCE9225-69AE-11D9-BED3-505054503030}"`，确保“成功”和“失败”均为“已启用”。

3. 若 **raw_os_error=10013** 且没有 5152：拦截可能发生在不产生丢包审计的层，或由在 WFP 之前拒绝连接的组件（例如能力检查）导致。继续排查 WFP 筛选器顺序和自定义 AppContainer 处理仍有帮助。

## 能力与环回（DNS / 本地解析器）

AppContainer 通常除 INTERNET_CLIENT 外还需要 **PRIVATE_NETWORK_CLIENT_SERVER** (S-1-15-3-3)，以便系统 DNS 解析器走本地网络路径；否则 getaddrinfo 可能以 11001 (WSAHOST_NOT_FOUND) 失败。Synbot 在启用网络时会添加 PRIVATE_NETWORK_CLIENT_SERVER。

若 DNS 仍失败，可尝试为 AppContainer 添加**环回豁免**（当 DNS 在环回上提供时有用，例如 127.0.0.53）。以**管理员**身份运行 PowerShell：

```powershell
CheckNetIsolation.exe LoopbackExempt -a -p=S-1-15-2-3537034781-887680828-4122948482-779883646-1262402909-2466852205-2671899347
```

将 `-p=` 后的 SID 替换为沙箱日志中的**实际** AppContainer SID（例如 `AppContainerSid=S-1-15-2-...`）。若系统使用包名，也可用：`CheckNetIsolation.exe LoopbackExempt -a -n=SynBot.Sandbox.synbot-app`（来自 CreateAppContainerProfile 的配置文件名）。查看当前豁免：`CheckNetIsolation.exe LoopbackExempt -s`。

## 参考

- [Diagnosing Network Isolation Issues (Microsoft)](https://techcommunity.microsoft.com/t5/core-infrastructure-and-security/diagnosing-network-isolation-issues/ba-p/2511562)
- [Event 5152 - Windows Filtering Platform blocked a packet](https://learn.microsoft.com/en-us/windows/security/threat-protection/auditing/event-5152)
