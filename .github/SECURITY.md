# 安全策略

权威安全语义（威胁模型、T 编号用例、签名取舍）单源在 [docs/SECURITY.md](../docs/SECURITY.md)——本文件仅作 GitHub 平台侧入口（Security 页 / 漏洞上报指引），内容冲突以权威源为准。

## 上报漏洞

请通过 **GitHub Security Advisories** 私密上报：仓库页 Security → Report a vulnerability。请勿在公开 issue / 讨论中张贴漏洞细节。

- 收到报告后 72 小时内确认，7 天内给出初步评估。
- 修复发布后在 Release notes 中致谢（除非你希望匿名）。

## 发行物完整性

GitHub Release 附 `checksums.txt`（SHA-256，覆盖各平台二进制与内嵌技能物，FR-11.7）：

```bash
shasum -a 256 --check checksums.txt --ignore-missing
```

不引入签名体系（GPG / sigstore）——取舍登记于 docs/SECURITY.md §7。
