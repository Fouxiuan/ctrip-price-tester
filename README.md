# 携程查价测试台

这是从主项目携程 Worker 查价链路中拆出的独立 Windows 测试应用。它不包含登录页，也不连接主系统后端或数据库，只在同事本机调用 OpenCLI 和 Chrome，验证携程酒店的价格日历与指定日期房型能否正常返回。

## 使用流程

1. 输入酒店名称、关键词、携程酒店 ID，或粘贴酒店详情页链接。
2. 从搜索结果中选择酒店；数字 ID 会直接进入该酒店。
3. 查看价格日历中的全部日期、每日最低价和整个区间的最低价日期。
4. 点击任一日期，继续核验该日可售房型及风控诊断证据。

价格日历优先读取携程 `ctGetHotelPriceCalendar` 接口，一次获取完整区间，避免逐日重复请求；接口详情不可用时会回退读取页面日历。

## 能判断什么

- **查价成功**：详情页、`getHotelRoomListInland` 房价接口和房型价格均正常返回。
- **暂无可售房**：接口结构正常，但当前酒店与日期没有可售价格；不应判断为封号。
- **疑似受限**：页面能打开，但没有房价接口，或接口返回验证/风控/异常结构。需要换一台正常机器用相同条件复测。
- **环境异常**：OpenCLI、Chrome 扩展或浏览器桥接未连接，当前结果不能用于判断封号。

“疑似受限”只是诊断信号，不能单独证明账号被封。测试前应打开 Chrome、启用 OpenCLI 扩展，并确保使用需要验证的携程登录状态和网络。

## 本地开发

```powershell
Set-Location ctrip-price-tester
pnpm install
pnpm runtime:prepare
pnpm tauri dev
```

`runtime:prepare` 会下载 Node.js 22.14.0，并把 `@jackwener/opencli@1.8.6` 安装到 `src-tauri/resources/node/`。运行时目录和构建产物不会提交到 Git。

## 构建同事安装包

```powershell
Set-Location ctrip-price-tester
pnpm install
pnpm build:tauri
```

NSIS 安装包输出在：

```text
src-tauri/target/release/bundle/nsis/
```

安装包内包含便携 Node.js 与 OpenCLI；同事仍需安装 Chrome 和 OpenCLI 浏览器扩展，并保持扩展连接正常。

## 自动更新

应用通过 GitHub Releases 自动更新（tauri-plugin-updater）。

### 签名密钥

公钥已写入 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`。**私钥保存在 `%USERPROFILE%\.tauri\ctrip-price-tester.key`，绝不能提交到仓库或泄露**；丢失私钥将无法再发布更新。

密钥对由以下命令生成：

```powershell
tauri signer generate -w "$env:USERPROFILE\.tauri\ctrip-price-tester.key"
```

### 发布新版本（带签名构建）

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY_PATH = "$env:USERPROFILE\.tauri\ctrip-price-tester.key"
pnpm build:tauri
```

构建产物（`nsis/` 目录）：

- `携程查价测试台_<版本>_x64-setup.exe` —— 安装版，支持自动更新
- `携程查价测试台_<版本>_x64-portable.exe` —— 便携版，不支持自动更新（检测到更新时打开 GitHub Releases 页面）

更新源指向 `https://github.com/Fouxiuan/ctrip-price-tester/releases/latest/download/latest.json`。

### 上传 Release

1. 在 GitHub 创建 tag（如 `v0.3.0`），版本号需高于当前 `package.json` / `tauri.conf.json` 的版本
2. 上传到该 Release：
   - `携程查价测试台_<版本>_x64-setup.exe`
   - `latest.json`（格式如下，文件名严格为 `latest.json` 才能被 `releases/latest/download/` 命中）

```json
{
  "version": "0.3.0",
  "notes": "更新说明（可选）",
  "pub_date": "2026-08-18T10:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "签名内容（构建日志会输出，或运行 tauri signer sign 生成）",
      "url": "https://github.com/Fouxiuan/ctrip-price-tester/releases/download/v0.3.0/携程查价测试台_0.3.0_x64-setup.exe"
    }
  }
}
```

签名生成（在构建目录下）：

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY_PATH = "$env:USERPROFILE\.tauri\ctrip-price-tester.key"
tauri signer sign -f .\携程查价测试台_0.3.0_x64-setup.exe
```

### 更新交互

- 启动 3 秒后自动检查更新；发现新版本弹出确认框
- 确认后显示下载进度条，下载完成自动安装并重启（`relaunch`）
- 便携版（portable）检测到更新时不走自动更新，直接打开 GitHub Releases 页面下载
