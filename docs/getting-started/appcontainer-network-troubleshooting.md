# AppContainer sandbox: outbound network not working

If the daemon runs inside the Windows AppContainer sandbox with network enabled but outbound HTTPS fails (e.g. Feishu/OpenAI "error sending request for url"), use the steps below to find which Windows Filtering Platform (WFP) filter is blocking the connection.

## First run (or after reboot): use Administrator once

Adding the firewall outbound rule and WFP permit for the AppContainer requires **Administrator** rights. The rules are **persistent**: they are **not** removed when the sandbox stops. So:

- **First run after install or after a reboot:** run `synbot sandbox` (or whatever starts the AppContainer daemon) **once as Administrator** (e.g. right-click → Run as administrator). The program will add the firewall and WFP rules; they remain in place after the sandbox stops.
- **Later runs:** any user (including normal user) can start the sandbox; the existing rules allow the AppContainer’s outbound network, so no Administrator is needed.
- **After a reboot:** WFP filters are cleared by Windows; firewall rules may remain. Run once as Administrator to re-add WFP (and any missing firewall rules), then normal user can run again. If rules were removed manually or by policy, run once as Administrator to re-add them.

## Turn off WFP packet-drop auditing (restore)

After troubleshooting, **disable** the WFP packet-drop audit to reduce Security log volume. Run PowerShell or CMD as **Administrator**:

```powershell
auditpol /set /subcategory:"{0CCE9225-69AE-11D9-BED3-505054503030}" /success:disable /failure:disable
```

To confirm it is off: `auditpol /get /subcategory:"{0CCE9225-69AE-11D9-BED3-505054503030}"` should show Success and Failure as disabled.

## Root cause: DNS (11001) vs WFP

If the daemon log shows **raw_os_error=Some(11001)** (WSAHOST_NOT_FOUND) or **dns error: proto error: no connections available**, the failure is **DNS** inside the AppContainer: the system resolver does not work there, and a resolver that reads system DNS has no nameservers (cannot read system DNS config). Synbot uses **hickory-dns** for reqwest and, in the startup diagnostic when inside AppContainer, a custom **GoogleDnsResolver** (Google 8.8.8.8 via `ResolverConfig::google()`) so the diagnostic request does not rely on system config. If you see `AppContainer network diagnostic: GET https://www.microsoft.com -> 200`, outbound HTTPS with explicit DNS works. The **open-lark (Feishu)** client still uses its default resolver; if it shows "no connections available", that confirms the system nameserver list is empty in the container. No 5152/5157 events appear for DNS failures.

## Confirm Capabilities and SID

From the sandbox log you should see:

- `CreateAppContainerProfile with 4 or 5 capability SIDs` (includes INTERNET_CLIENT and PRIVATE_NETWORK_CLIENT_SERVER when network is enabled)
- `SECURITY_CAPABILITIES: CapabilityCount=4` (or 5)
- `AppContainerSid=S-1-15-2-...` (same in parent and child)

If any of these are missing or zero, fix the token/capabilities first. If they are correct, the block is at WFP.

## Find the blocking WFP filter

1. **Enable WFP packet-drop auditing** (run PowerShell or CMD as Administrator):

   Use the subcategory **GUID** (avoids "parameter error" on non-English Windows, e.g. when the display name is localized):

   ```powershell
   auditpol /set /subcategory:"{0CCE9225-69AE-11D9-BED3-505054503030}" /success:enable /failure:enable
   ```

   If that fails, list subcategories to get your system's name (e.g. 筛选平台数据包丢弃 on Chinese Windows):

   ```powershell
   auditpol /list /subcategory:*
   ```

   Then use the exact name in quotes, e.g. `auditpol /set /subcategory:"筛选平台数据包丢弃" /success:enable /failure:enable`.

2. **Reproduce the failure**: start synbot with the sandbox and trigger Feishu (or any outbound HTTPS).

3. **Open Event Viewer** → Windows Logs → **Security**. Filter by **Event ID 5152** and **5157**, time range = when you ran synbot. Look for:
   - **Task category**: Filtering Platform Packet Drop  
   - **Direction**: Outbound  
   - **Source/Destination**: your machine → open.feishu.cn (or the target IP)

4. In the event details, note **Filter Run-Time ID** and **Layer Run-Time ID**. The Filter Run-Time ID is the filter that blocked the packet.

5. **Match the filter** to your WFP state:
   - Export state: `netsh wfp show state file=wfpstate.xml` (Admin).
   - Open `wfpstate.xml` and search for `<filterId>RUN_TIME_ID</filterId>` (replace RUN_TIME_ID with the value from the event). The surrounding `<displayData><name>...</name>` is the filter name; `<layerKey>` is the layer.

6. **Interpret**:
   - If the blocking filter is **SynBot AppContainer Outbound V4/V6**, our permit filter is present but not matching (e.g. condition or SID mismatch).
   - If the blocking filter is from **MPSSVC** / **App Isolation** or similar, a system rule is denying the custom AppContainer; you may need to adjust our filter weight/sublayer or investigate OS/security baseline.

7. **Turn off auditing** when done (optional, to reduce log volume):

   ```powershell
   auditpol /set /subcategory:"{0CCE9225-69AE-11D9-BED3-505054503030}" /success:disable /failure:disable
   ```

## If Security log has no 5152/5157 (no WFP drop events)

If auditing is enabled but you see **no** outbound block events for the AppContainer process, the failure may be **before** WFP (e.g. Winsock/token) or in a path that does not log 5152.

1. **Check the daemon log** for the one-shot line that runs at startup when inside AppContainer:
   - `AppContainer network diagnostic: GET https://www.microsoft.com -> 200` → outbound worked at startup (failure may be later or Feishu-specific).
   - `AppContainer network diagnostic: request failed io_error kind=... raw_os_error=...` → note **raw_os_error**:
- **11001** (WSAHOST_NOT_FOUND) = DNS resolution failed in AppContainer (use reqwest with **hickory-dns** feature; see “Root cause: DNS (11001)” above).
   - **10013** (WSAEACCES) = access denied (often token or firewall before WFP, or WFP block without audit).
   - **10061** (WSAECONNREFUSED) = connection refused by server.
   - **10060** (WSAETIMEDOUT) = timeout.
   - `AppContainer network diagnostic: request timed out after 5s` → no reply in 5s (could be block or network/DNS).

2. **Confirm audit is on**: run `auditpol /get /subcategory:"{0CCE9225-69AE-11D9-BED3-505054503030}"` and ensure Success and Failure are enabled.

3. If **raw_os_error=10013** and no 5152: the block may be in a layer that does not emit packet-drop audit, or by a component that denies the connection before WFP (e.g. capability check). Continuing to investigate WFP filter ordering and custom AppContainer handling may still help.

## Capabilities and Loopback (DNS / local resolver)

AppContainer often needs **PRIVATE_NETWORK_CLIENT_SERVER** (S-1-15-3-3) in addition to INTERNET_CLIENT so the system DNS resolver can use the local network path; otherwise getaddrinfo may fail with 11001 (WSAHOST_NOT_FOUND). Synbot adds PRIVATE_NETWORK_CLIENT_SERVER whenever network is enabled.

If DNS still fails, you can try adding a **loopback exemption** for the AppContainer (helps when DNS is served on loopback, e.g. 127.0.0.53). Run PowerShell as **Administrator**:

```powershell
CheckNetIsolation.exe LoopbackExempt -a -p=S-1-15-2-3537034781-887680828-4122948482-779883646-1262402909-2466852205-2671899347
```

Replace the `-p=` SID with the **exact** AppContainer SID from your sandbox log (e.g. `AppContainerSid=S-1-15-2-...`). Alternatively, if your system uses a package name: `CheckNetIsolation.exe LoopbackExempt -a -n=SynBot.Sandbox.synbot-app` (profile name from CreateAppContainerProfile). List current exemptions: `CheckNetIsolation.exe LoopbackExempt -s`.

## References

- [Diagnosing Network Isolation Issues (Microsoft)](https://techcommunity.microsoft.com/t5/core-infrastructure-and-security/diagnosing-network-isolation-issues/ba-p/2511562)
- [Event 5152 - Windows Filtering Platform blocked a packet](https://learn.microsoft.com/en-us/windows/security/threat-protection/auditing/event-5152)
