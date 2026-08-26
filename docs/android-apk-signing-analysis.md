# Android APK Signing Scheme 深度分析

**目的：** 为 SOL 签名机制设计提供技术参考

**来源：** Android 官方文档 + AOSP 源码分析

---

## 1. 演进历史

| 版本 | 引入时间 | 核心改进 | 主要问题 |
|------|----------|----------|----------|
| **v1 (JAR signing)** | Android 1.0 (2008) | 基于 JAR 签名，每个文件单独签名 | 验证慢、可篡改 META-INF/、不保护 ZIP 结构 |
| **v2** | Android 7.0 (2016) | 全内容签名、APK Signing Block | 不支持密钥轮换 |
| **v3** | Android 9.0 (2018) | **密钥轮换（lineage）** | - |
| **v4** | Android 11 (2020) | 流式验证、增量更新 | 需要 v2/v3 基础 |

---

## 2. V1 签名机制（JAR Signing）

### 2.1 基本结构
```text
app.apk (ZIP 格式)
├── META-INF/
│   ├── MANIFEST.MF      # 文件摘要列表
│   ├── CERT.SF          # 签名文件（对 MANIFEST.MF 的签名）
│   └── CERT.RSA         # 证书 + 对 CERT.SF 的签名
├── AndroidManifest.xml
├── classes.dex
├── res/
└── ...
```

### 2.2 签名流程
```text
1. 计算每个文件的 SHA-256
   AndroidManifest.xml → SHA-256: abc123...
   classes.dex → SHA-256: def456...

2. 写入 MANIFEST.MF:
   Name: AndroidManifest.xml
   SHA-256-Digest: abc123...
   
   Name: classes.dex
   SHA-256-Digest: def456...

3. 对 MANIFEST.MF 整体签名 → CERT.SF:
   Signature-Version: 1.0
   SHA-256-Digest-Manifest: xyz789...
   
   Name: AndroidManifest.xml
   SHA-256-Digest: <hash of manifest entry>

4. 用私钥签名 CERT.SF + 附加证书 → CERT.RSA
```

### 2.3 验证流程
```text
1. 用公钥验证 CERT.RSA → 得到 CERT.SF
2. 验证 CERT.SF 中的摘要与 MANIFEST.MF 一致
3. 逐个验证文件的 SHA-256 与 MANIFEST.MF 中记录一致
```

### 2.4 致命缺陷

**问题 1：META-INF/ 不受保护**
```text
攻击者可以添加 META-INF/evil.jar，因为 v1 不会为 META-INF/ 自身生成摘要
→ 可以注入恶意代码到 META-INF/services/ 等目录
```

**问题 2：ZIP 结构不受保护**
```text
ZIP 格式允许：
- 文件注释
- 额外的 Central Directory 条目
- 未被索引的数据

攻击者可以在 ZIP 层面添加数据，v1 签名无法检测
```

**问题 3：性能问题**
```text
安装时需要解压并验证每个文件 → 大型 APK 验证耗时数秒
```

**问题 4：Janus 漏洞（CVE-2017-13156）**
```text
ZIP 和 DEX 文件格式都允许文件头前有额外数据
→ 可以构造一个文件，同时是合法的 ZIP（APK）和 DEX
→ 签名验证看到的是 ZIP，运行时加载的是 DEX
```

---

## 3. V2 签名机制（APK Signature Scheme v2）

### 3.1 核心设计：APK Signing Block

```text
APK 文件结构（ZIP with Signing Block）:

┌─────────────────────────────────┐
│  ZIP Entries (contents)         │  ← 原始文件内容
│  - AndroidManifest.xml          │
│  - classes.dex                  │
│  - res/*                        │
├─────────────────────────────────┤
│  APK Signing Block ★            │  ← v2/v3 签名数据（新增）
│  - size of block                │
│  - ID-value pairs:              │
│    - 0x7109871a: v2 signature   │
│    - 0xf05368c0: v3 signature   │
│  - magic "APK Sig Block 42"     │
│  - size of block (again)        │
├─────────────────────────────────┤
│  Central Directory              │  ← ZIP 索引
├─────────────────────────────────┤
│  End of Central Directory       │  ← ZIP 结束标记
│  (offset 指向 Central Dir)      │
└─────────────────────────────────┘
```

**关键技巧：**
- Signing Block 插在 ZIP 内容和 Central Directory 之间
- 修改 EOCD 中的 Central Directory offset，使其跳过 Signing Block
- 旧版 ZIP 工具会忽略 Signing Block（向后兼容）
- 新版签名验证器强制要求 Signing Block 存在

### 3.2 签名数据结构

```text
APK Signing Block ID-Value 格式:

┌─────────────────────────────────┐
│ uint64: size of block           │
├─────────────────────────────────┤
│ Repeated ID-Value pairs:        │
│                                 │
│ ┌─────────────────────────────┐ │
│ │ uint64: size                │ │
│ │ uint32: ID                  │ │  0x7109871a = v2 signature
│ │ bytes: value                │ │
│ └─────────────────────────────┘ │
│                                 │
│ ┌─────────────────────────────┐ │
│ │ uint64: size                │ │
│ │ uint32: ID                  │ │  0xf05368c0 = v3 signature
│ │ bytes: value                │ │
│ └─────────────────────────────┘ │
├─────────────────────────────────┤
│ uint64: size of block (again)   │
├─────────────────────────────────┤
│ bytes: magic "APK Sig Block 42" │
└─────────────────────────────────┘
```

### 3.3 V2 Signature Block 内部结构

```protobuf
// v2 signature block (ID 0x7109871a)
message ApkSignatureSchemeV2Block {
  repeated Signer signers = 1;
}

message Signer {
  bytes signed_data = 1;       // 被签名的数据
  repeated Signature signatures = 2;  // 一个或多个签名
  bytes public_key = 3;        // 公钥（DER 编码）
}

message SignedData {
  repeated Digest digests = 1;  // APK 内容摘要（多种算法）
  repeated Certificate certificates = 2;  // X.509 证书链
  repeated AdditionalAttribute additional_attributes = 3;
}

message Digest {
  uint32 signature_algorithm_id = 1;  // 0x0101 = RSA with SHA-256
  bytes digest = 2;                   // 摘要值
}
```

### 3.4 V2 签名流程

```text
1. 将 APK 分为 4 个区域:
   - ZIP entries (contents)
   - APK Signing Block (excluding v2 signature)
   - Central Directory
   - End of Central Directory

2. 计算每个区域的摘要（支持多种算法同时使用）:
   SHA-256(区域1) || SHA-256(区域2) || SHA-256(区域3) || SHA-256(区域4)
   
3. 构造 SignedData:
   - digests: 包含上述摘要
   - certificates: 签名证书链
   
4. 用私钥签名 SignedData → Signature
   
5. 构造 Signer:
   - signed_data: SignedData 序列化
   - signatures: 上述 Signature
   - public_key: 公钥
   
6. 将 Signer 打包成 v2 block，插入 APK Signing Block
```

### 3.5 V2 验证流程

```text
1. 检查 APK Signing Block 是否存在
2. 找到 ID 0x7109871a 的 v2 signature block
3. 提取所有 Signer
4. 对每个 Signer:
   a. 用 public_key 验证 signatures → 得到 signed_data
   b. 从 signed_data 提取 digests
   c. 重新计算 APK 4 个区域的摘要
   d. 比较计算值与 signed_data 中的 digests
   e. 验证证书链有效性
5. 至少一个 Signer 验证成功 → APK 合法
```

### 3.6 V2 的改进

✅ **全内容保护**：ZIP entries、Central Directory、EOCD 全部覆盖  
✅ **快速验证**：只需计算 4 个区域的摘要，无需解压  
✅ **防篡改**：任何修改都会导致摘要不匹配  
✅ **多签名**：支持多个 Signer（企业场景）  

### 3.7 V2 的局限

❌ **无密钥轮换**：换密钥 = 新应用，用户需重新授权  
❌ **不支持增量更新**：必须验证完整 APK  

---

## 4. V3 签名机制（APK Signature Scheme v3）

### 4.1 核心改进：Proof-of-Rotation（密钥轮换证明）

V3 = V2 + **Lineage（血统）** 结构

```text
APK Signing Block:
├── 0x7109871a: v2 signature block
├── 0xf05368c0: v3 signature block ★
└── ...

v3 block 内部:
┌─────────────────────────────────┐
│ Signer (与 v2 类似)             │
│ ├── signed_data                 │
│ ├── signatures                  │
│ └── public_key                  │
├─────────────────────────────────┤
│ Proof-of-Rotation ★             │  ← 新增：密钥轮换历史
│ ├── SignerConfig (Key A)        │
│ │   ├── certificate             │
│ │   ├── signature (签名下一个key)│
│ │   └── signedData (metadata)   │
│ ├── SignerConfig (Key B)        │
│ │   ├── certificate             │
│ │   ├── signature               │
│ │   └── signedData              │
│ └── SignerConfig (Key C - 当前) │
└─────────────────────────────────┘
```

### 4.2 Proof-of-Rotation 数据结构

```protobuf
message ProofOfRotation {
  repeated SignerConfig signers = 1;
}

message SignerConfig {
  bytes certificate = 1;         // X.509 证书
  bytes signed_data = 2;         // 被签名的数据
  repeated Signature signatures = 3;  // 对 signed_data 的签名
}

message SignedData {
  bytes certificate_of_next_key = 1;  // 下一个密钥的证书
  uint32 signature_algorithm_id = 2;
  
  // 权限标志（Android 特有）
  bool granted_permissions = 3;  // 旧 key 是否授权给新 key
  uint32 min_sdk_version = 4;    // 最低 SDK 版本
}
```

### 4.3 密钥轮换语义

```text
Lineage: [Key A] → [Key B] → [Key C (current)]

Key A 签名: certificate_of_next_key = Key B 的证书
           signature = Sign(Key A, Key B's cert + metadata)

Key B 签名: certificate_of_next_key = Key C 的证书
           signature = Sign(Key B, Key C's cert + metadata)

Key C: 当前活跃密钥，签名当前 APK
```

**验证时：**
```text
1. 当前 APK 由 Key C 签名 ✓
2. Key C 由 Key B 签名（lineage 中）✓
3. Key B 由 Key A 签名（lineage 中）✓
4. Key A 是初始信任锚点（首次安装时记录）✓

→ Key C 是 Key A 的合法继承者 → 保留应用权限和数据
```

### 4.4 实际案例

**场景：Google Play Games 密钥轮换**

```text
初始安装（2015）:
  Package: com.google.android.play.games
  Key: RSA-2048 key A (expires 2025)
  → 系统记录: trusted_key = Key A

更新（2023，密钥即将过期）:
  APK 签名: Key B (RSA-4096, expires 2045)
  Lineage: [Key A] → [Key B]
  → 系统验证:
    - Key B 签名有效 ✓
    - Lineage 中 Key A 签名了 Key B ✓
    - Key A == trusted_key ✓
  → 更新成功，trusted_key 更新为 Key B

用户不感知，数据和权限无缝继承
```

### 4.5 Android 特有：Per-SDK-Version Rotation

```protobuf
message SignedData {
  uint32 min_sdk_version = 4;  // 例如：28 (Android 9.0)
}
```

**用途：** 限制旧密钥的使用范围

```text
Lineage:
  Key A (min_sdk = 1)  → 可验证 Android 1.0+ 设备
  Key B (min_sdk = 28) → 只能验证 Android 9.0+ 设备

场景：Key A 被泄露
  → 发布 emergency update，Lineage 中标记 Key A min_sdk = 999999
  → 所有设备拒绝接受 Key A 签名的旧版本
```

**SOL 不需要这个**（没有碎片化的 SDK 版本问题）

---

## 5. V4 签名机制（APK Signature Scheme v4）

### 5.1 设计目标

**问题：** V2/V3 必须下载完整 APK 才能验证签名  
**场景：** 大型游戏（5GB+），用户等待数分钟只为验证签名

**V4 目标：** 支持流式验证和增量更新

### 5.2 核心技术：Merkle Hash Tree + fsverity

```text
V4 不替代 V2/V3，而是补充:

APK 文件:
├── v2/v3 signature (完整签名，用于首次安装)
└── ...

独立的 .apk.idsig 文件:
├── Merkle tree root hash
├── File hash tree
└── v4 signature
```

### 5.3 Merkle Tree 结构

```text
将 APK 分块（4KB 块）:

┌─────────────────────────────────────────┐
│        APK Content (分块)               │
├──────┬──────┬──────┬──────┬──────┬──────┤
│Block0│Block1│Block2│Block3│Block4│Block5│
│ 4KB  │ 4KB  │ 4KB  │ 4KB  │ 4KB  │ 4KB  │
└──┬───┴──┬───┴──┬───┴──┬───┴──┬───┴──┬───┘
   H0     H1     H2     H3     H4     H5   ← Level 0（叶子哈希）
    └──┬───┘      └──┬───┘      └──┬───┘
      H01           H23           H45      ← Level 1
       └──────┬──────┘             │
             H0123 ─────────────── H45     ← Level 2
                    └────┬─────┘
                      Root Hash            ← Level 3（根哈希）
                         ↓
                   Sign(Root Hash)         ← V4 签名
```

### 5.4 流式验证流程

```text
下载 APK 时（增量验证）:

1. 先下载 .apk.idsig（几百 KB）
2. 验证 v4 signature → 得到可信的 Root Hash
3. 边下载边验证:
   - 下载 Block 0 → 计算 H0 → 验证到 Root Hash
   - 下载 Block 1 → 计算 H1 → 验证到 Root Hash
   - ...
4. 任意块验证失败 → 立即中止下载

好处：
  - 不需要下载完整 APK 才开始验证
  - 增量更新时只需重新验证变化的块
  - 支持 Google Play Instant（渐进式下载游戏）
```

### 5.5 与 Linux fsverity 的关系

Android 11+ 使用 kernel fsverity 加速验证：

```c
// 将 APK Merkle tree 注册到内核
ioctl(fd, FS_IOC_ENABLE_VERITY, &merkle_tree);

// 后续读取自动验证，内核返回错误如果块被篡改
read(fd, buffer, size);  // 内核自动验证哈希
```

**好处：**
- 验证由内核完成，性能更高
- 读取时验证，而非安装时验证（lazy verification）
- 防止已安装 APK 被篡改（只读保护）

### 5.6 .apk.idsig 文件格式

```text
.apk.idsig 文件:

┌─────────────────────────────────┐
│ Magic: "IDSIG"                  │
├─────────────────────────────────┤
│ Version: 2                      │
├─────────────────────────────────┤
│ Hashing info:                   │
│ - hash_algorithm (SHA-256)      │
│ - log2_block_size (12 = 4KB)    │
│ - salt (32 bytes)               │
├─────────────────────────────────┤
│ Merkle tree (Level 0 → Root):   │
│ [H0, H1, H2, ..., Root Hash]    │
├─────────────────────────────────┤
│ V4 Signature:                   │
│ - algorithm                     │
│ - signature (over Root Hash)    │
│ - public_key                    │
│ - certificate                   │
└─────────────────────────────────┘
```

---

## 6. 多版本共存策略

### 6.1 验证优先级

```text
Android 系统验证逻辑:

if (apk.hasV4Signature() && device.supportsV4()) {
    verify_v4();  // 流式验证
}

// V4 不能单独存在，必须有 v2 或 v3 基础
if (apk.hasV3Signature()) {
    verify_v3();  // 验证签名 + lineage
} else if (apk.hasV2Signature()) {
    verify_v2();  // 验证签名
} else {
    verify_v1();  // 回退到 v1（不推荐）
}
```

### 6.2 向后兼容

```text
为了兼容旧设备，现代 APK 通常包含多版本签名:

app.apk:
├── META-INF/ (v1 signature)
├── APK Signing Block:
│   ├── v2 signature
│   └── v3 signature
└── app.apk.idsig (v4, 单独文件)

Android 7+: 使用 v2
Android 9+: 使用 v3
Android 11+: 使用 v4 + v3
Android 6-: 使用 v1
```

---

## 7. 安全性分析

### 7.1 V1 已知攻击

| 攻击 | 原理 | 影响 |
|------|------|------|
| **Janus** (CVE-2017-13156) | DEX/ZIP 双重格式 | 代码注入 |
| **Master Key** (CVE-2013-4787) | ZIP 重复文件名 | 替换 classes.dex |
| **META-INF injection** | v1 不保护 META-INF/ | 注入恶意服务 |

### 7.2 V2/V3/V4 防护

✅ **全内容覆盖** → Janus 无效  
✅ **ZIP 结构保护** → Master Key 无效  
✅ **Signing Block 保护** → META-INF injection 无效  
✅ **密钥轮换** (v3) → 密钥过期/泄露可恢复  
✅ **增量验证** (v4) → 大文件不再是性能瓶颈  

### 7.3 Lineage 安全边界

**安全场景：**
```text
正常轮换: Key A → Key B → Key C
  → 用户安装 APK signed by Key A
  → 更新到 APK signed by Key C (lineage present)
  → 权限保留 ✓
```

**不安全场景（系统拒绝）：**
```text
攻击者偷到 Key C 但没有 Key B:
  → 无法构造完整 lineage [A → B → C]
  → 只能构造新 lineage [C]
  → 系统检测到不连续 → 拒绝安装 ✓
```

**攻击者偷到整个 lineage：**
```text
如果攻击者获取了完整的 lineage + 当前私钥:
  → 可以签名新版本，系统无法区分
  → 这是密钥管理问题，签名方案无法解决
  → 解决方案：硬件密钥（HSM）、定期轮换、快速撤销
```

---

## 8. SOL 的借鉴与改进

### 8.1 从 Android 借鉴

✅ **V3 lineage 机制**：完整采用，适配 SOL 权限模型  
✅ **V2 全内容签名**：保护 bundle 完整性  
✅ **多签名支持**：企业场景（公司合并等）  

### 8.2 SOL 的改进

**1. 算法选择**
```text
Android: 默认 RSA-2048（历史原因）
SOL:     默认 Ed25519（2024+ 标准）
  → 签名更小（64 bytes vs 256 bytes）
  → 验证更快（~4x faster）
  → 安全性更高（无 padding oracle）
```

**2. 去除 Android 特有复杂性**
```text
不需要：
  ❌ Per-SDK-version rotation (SOL 无碎片化)
  ❌ V1 兼容（SOL 从 v2+ 起步）
  ❌ Shareduid (Android legacy feature)
  ❌ AndroidManifest.xml 特殊处理
```

**3. Repository 双层签名**
```text
Android: 只有 APK 签名，Play Store 是中心化信任锚点
SOL:     APK 签名 + Repository 签名
  → Repository 签名包含撤销状态
  → 支持多仓库（官方 + 第三方）
  → 离线验证友好
```

**4. 密钥轮换可审计**
```text
Android: Lineage 只在 APK 内部
SOL:     Lineage 同时发布到公开日志（可选）
  → 用户可审计密钥轮换历史
  → 检测非预期的密钥变更
  → 类似 Certificate Transparency
```

### 8.3 V4 流式验证的取舍

**Android 需要 V4 的原因：**
- Google Play Instant（边下边玩）
- 5GB+ 大型游戏常见
- 移动网络不稳定

**SOL 的选择：**
```text
Phase 8: 不实现 V4（复杂度高）
  → 桌面环境网络稳定
  → 应用通常 < 500MB
  → 可以等完整下载后验证

Phase 9+: 按需评估
  → 如果出现大型游戏/创意应用
  → 可以引入类似机制
  → 优先考虑 FS-verity 内核集成
```

---

## 9. 实现清单

### 9.1 Phase 8.1 基础签名（必须）

- [ ] MANIFEST.json 生成（内容摘要树）
- [ ] Ed25519 签名/验证
- [ ] ECDSA P-256 签名/验证（兼容性）
- [ ] RSA-4096 验证（legacy only）
- [ ] 签名 block 格式（类似 v2）
- [ ] 多签名支持
- [ ] 证书链验证（可选 X.509）

### 9.2 Phase 8.2 Lineage（必须）

- [ ] Lineage protobuf 格式
- [ ] 密钥轮换工具 (`sol-bundle rotate-key`)
- [ ] Lineage 验证逻辑
- [ ] 权限继承判定（same lineage vs discontinuous）
- [ ] 与 `sol-securityd` 集成

### 9.3 Phase 8.3 高级特性（可选）

- [ ] Repository 元数据签名
- [ ] 撤销检查（CRL/OCSP-style）
- [ ] 硬件密钥支持（TPM/Yubikey）
- [ ] Transparency log（公开轮换历史）

### 9.4 不实现（明确排除）

- ❌ V1 JAR signing（不需要向后兼容）
- ❌ V4 streaming verification（Phase 8 不需要）
- ❌ Per-SDK-version rotation（SOL 无此概念）
- ❌ Shareduid（Android legacy）

---

## 10. 测试矩阵

### 10.1 基础签名测试

| 测试 | 预期结果 |
|------|----------|
| Ed25519 签名 + 验证 | ✓ |
| ECDSA P-256 签名 + 验证 | ✓ |
| RSA-4096 验证 | ✓ |
| 篡改内容后验证 | ✗ 拒绝 |
| 篡改签名后验证 | ✗ 拒绝 |
| 过期密钥验证 | ✗ 拒绝 |
| 多签名（2 个有效密钥） | ✓ |
| 多签名（1 个有效 + 1 个无效） | ✗ 拒绝 |

### 10.2 Lineage 测试

| 测试 | 预期结果 |
|------|----------|
| 单次轮换 A→B | ✓ 权限继承 |
| 三次轮换 A→B→C→D | ✓ 权限继承 |
| 断裂血统（无 A→B 签名） | ✗ 新身份 |
| 回滚到旧密钥验证旧版本 | ✓ |
| 当前密钥不在 lineage 中 | ✗ 拒绝 |
| 攻击者用新 root key 签名 | ✗ 不继承权限 |

### 10.3 性能测试

| 场景 | 目标 | Android 基准 |
|------|------|--------------|
| 验证 10MB bundle | < 50ms | v2: ~30ms |
| 验证 100MB bundle | < 200ms | v2: ~150ms |
| 验证 1GB bundle | < 2s | v2: ~1.5s |
| 生成签名 | < 100ms | v2: ~80ms |

---

## 11. 参考资源

- [Android Source - APK Signature Scheme v2](https://source.android.com/docs/security/features/apksigning/v2)
- [Android Source - APK Signature Scheme v3](https://source.android.com/docs/security/features/apksigning/v3)
- [Android Source - APK Signature Scheme v4](https://source.android.com/docs/security/features/apksigning/v4)
- [AOSP: apksig Library](https://android.googlesource.com/platform/tools/apksig/)
- [APK Signature Scheme v2 Verification](https://source.android.com/docs/security/features/apksigning/v2#verification)
- [Janus Vulnerability Explained](https://www.guardsquare.com/blog/new-android-vulnerability-allows-attackers-modify-apps-without-affecting-their-signatures)

---

**总结：** Android 的签名演进提供了宝贵的经验。SOL 应采用 v2/v3 的核心设计（全内容签名 + lineage），但简化不必要的复杂性，使用现代算法（Ed25519），并增加透明度机制。V4 的流式验证暂不需要，但架构上应保留扩展性。
