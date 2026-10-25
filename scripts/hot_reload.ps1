# RuhI 通用智慧型輸入法：全自動免登出熱更新 (Hot Reload & In-Process Live Swap)
# 符合 ATG 全域守則：原生 UTF-8 BOM、動態路徑解耦、try-finally 守護 ctfmon、無須管理員權限
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "  正在執行 RuhI TSF 全自動免登出熱更新 (Hot Reload)..." -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

# 1. 動態推導專案根目錄
$scriptDir = $PSScriptRoot
$rootDir = $scriptDir
if (-not (Test-Path (Join-Path $rootDir "ruhi-tsf"))) {
    if (Test-Path (Join-Path $scriptDir "..\ruhi-tsf")) {
        $rootDir = (Resolve-Path (Join-Path $scriptDir "..")).Path
    }
}

Write-Host "  專案根目錄: $rootDir" -ForegroundColor DarkGray

# 2. 尋找最新編譯之 Release DLL
$sourceDllCandidates = @(
    (Join-Path $rootDir "ruhi-tsf\target\release\ruhi_tsf.dll"),
    (Join-Path $rootDir "target\release\ruhi_tsf.dll")
)

$sourceDll = $null
foreach ($cand in $sourceDllCandidates) {
    if (Test-Path $cand) {
        $sourceDll = (Resolve-Path $cand).Path
        break
    }
}

if (-not $sourceDll) {
    Write-Host ""
    Write-Host "  【錯誤】找不到編譯好的 Release DLL！" -ForegroundColor Red
    Write-Host "  請先在終端機執行: cargo build --release --manifest-path ruhi-tsf/Cargo.toml" -ForegroundColor Yellow
    Write-Host ""
    exit 1
}

$sourceTime = (Get-Item $sourceDll).LastWriteTime
Write-Host "  [1/4] 找到最新 Release DLL: $sourceDll" -ForegroundColor Green
Write-Host "        建置時間: $sourceTime" -ForegroundColor DarkGray

# 3. 準備部署目標與 InprocServer32 版本化熱替換路徑（破除既有進程 PEB->Ldr 同名模組快取）
$timestamp = Get-Date -Format 'yyyyMMdd_HHmmss'
$scriptsDir = Join-Path $rootDir "scripts"
if (-not (Test-Path $scriptsDir)) {
    New-Item -ItemType Directory -Path $scriptsDir -Force | Out-Null
}
$versionedDll = Join-Path $scriptsDir "ruhi_tsf_$timestamp.dll"

$targetPaths = [System.Collections.Generic.List[string]]::new()
$defaultScripts = Join-Path $scriptsDir "ruhi_tsf.dll"
$defaultDist = Join-Path $rootDir "dist\ruhi_tsf.dll"
$defaultRoot = Join-Path $rootDir "ruhi_tsf.dll"

foreach ($p in @($versionedDll, $defaultScripts, $defaultDist, $defaultRoot)) {
    if (-not $targetPaths.Contains($p)) {
        $targetPaths.Add($p)
    }
}

Write-Host "  [2/4] 目標部署路徑 ($($targetPaths.Count) 處):" -ForegroundColor Green
foreach ($t in $targetPaths) {
    Write-Host "        -> $t" -ForegroundColor DarkGray
}

$updatedCount = 0

# 4. 執行核心熱替換程序（以 try-finally 鐵律絕對守護 ctfmon 生命週期）
try {
    try {
        Get-Process -Name ctfmon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
        Write-Host "  [3/4] 已暫停文字服務框架監聽 (ctfmon)..." -ForegroundColor Green
    } catch {}
    Start-Sleep -Milliseconds 200

    # 清理 scripts 目錄下未被進程鎖定的舊版時間戳 DLL 與 .old 檔案
    Get-ChildItem -Path $scriptsDir -Filter "ruhi_tsf_*.dll" -ErrorAction SilentlyContinue | ForEach-Object {
        if ($_.FullName -ne $versionedDll) {
            Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue
        }
    }

    foreach ($target in $targetPaths) {
        $parent = Split-Path -Parent $target
        if (-not (Test-Path $parent)) {
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
        }

        if (Test-Path $target) {
            $leafName = Split-Path -Leaf $target
            Get-ChildItem -Path $parent -Filter "$leafName*.old" -ErrorAction SilentlyContinue | ForEach-Object {
                Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue
            }

            $renamed = $false
            $candidateOldNames = @(
                "$target.old",
                "$target.$timestamp.old"
            )

            foreach ($oldName in $candidateOldNames) {
                try {
                    Rename-Item -Path $target -NewName (Split-Path -Leaf $oldName) -Force -ErrorAction Stop
                    $renamed = $true
                    break
                } catch {}
            }
        }

        try {
            Copy-Item -Path $sourceDll -Destination $target -Force -ErrorAction Stop
            Write-Host "        [替換成功] $target" -ForegroundColor Green
            $updatedCount++
        } catch {
            Write-Host "        [警告] 拷貝失敗: $target ($($_.Exception.Message))" -ForegroundColor Yellow
        }

        # 同步完整詞庫與偏好設定至該目標目錄
        $targetDataDir = Join-Path $parent "data"
        $sourceDataDir = Join-Path $rootDir "ruhi_lexicon\data"
        if (Test-Path $sourceDataDir) {
            if (-not (Test-Path $targetDataDir)) {
                New-Item -ItemType Directory -Path $targetDataDir -Force | Out-Null
            }
            Get-ChildItem -Path $sourceDataDir -Recurse -File -Include "*.tsv", "*.jsonl", "*.json" -ErrorAction SilentlyContinue | ForEach-Object {
                $relPart = $_.FullName.Substring($sourceDataDir.Length).TrimStart('\', '/')
                $destSubFile = Join-Path $targetDataDir $relPart
                $destSubParent = Split-Path -Parent $destSubFile
                if (-not (Test-Path $destSubParent)) { New-Item -ItemType Directory -Path $destSubParent -Force | Out-Null }
                if ((-not (Test-Path $destSubFile)) -or ($_.LastWriteTime -gt (Get-Item $destSubFile).LastWriteTime)) {
                    Copy-Item $_.FullName -Destination $destSubFile -Force -ErrorAction SilentlyContinue
                }
            }
        }

        # 同步純 Rust 偏好設定與語意調校執行檔 (ruhi-studio.exe)
        $studioSrcCandidates = @(
            (Join-Path $rootDir "ruhi-studio\target\release\ruhi-studio.exe"),
            (Join-Path $rootDir "ruhi-studio.exe")
        )
        foreach ($sc in $studioSrcCandidates) {
            if (Test-Path $sc) {
                foreach ($dstStudio in @(
                    (Join-Path $parent "ruhi-studio.exe"),
                    (Join-Path $rootDir "ruhi-studio.exe"),
                    (Join-Path $rootDir "dist\ruhi-studio.exe")
                )) {
                    if ($sc -ne $dstStudio) {
                        try {
                            Copy-Item $sc -Destination $dstStudio -Force -ErrorAction Stop
                        } catch {
                            try {
                                Rename-Item -Path $dstStudio -NewName "$(Split-Path -Leaf $dstStudio).$timestamp.old" -Force -ErrorAction Stop
                                Copy-Item $sc -Destination $dstStudio -Force -ErrorAction Stop
                            } catch {}
                        }
                    }
                }
                break
            }
        }
        $icoSrc = Join-Path $rootDir "assets\ruhi.ico"
        if (Test-Path $icoSrc) {
            foreach ($icoDst in @((Join-Path $rootDir "ruhi.ico"), (Join-Path $parent "ruhi.ico"))) {
                try { Copy-Item $icoSrc -Destination $icoDst -Force -ErrorAction SilentlyContinue } catch {}
            }
        }
        foreach ($sf in @("settings_config.json", "config.json")) {
            $srcF = Join-Path $rootDir $sf
            if (Test-Path $srcF) {
                $dstF = Join-Path $parent $sf
                if ($srcF -ne $dstF) {
                    Copy-Item $srcF -Destination $dstF -Force -ErrorAction SilentlyContinue
                }
            }
        }
    }

    # 更新 HKCU InprocServer32 指向全新時間戳 DLL，並寫入 active_dll_path.txt 供進程內無感轉發
    if (Test-Path $versionedDll) {
        $hkcuClsid = "HKCU:\Software\Classes\CLSID\{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}\InprocServer32"
        if (-not (Test-Path $hkcuClsid)) {
            New-Item -Path $hkcuClsid -Force | Out-Null
        }
        Set-ItemProperty -Path $hkcuClsid -Name "(default)" -Value $versionedDll
        Set-ItemProperty -Path $hkcuClsid -Name "ThreadingModel" -Value "Apartment"
        [System.IO.File]::WriteAllText((Join-Path $scriptsDir "active_dll_path.txt"), $versionedDll, [System.Text.Encoding]::UTF8)
        [System.IO.File]::WriteAllText((Join-Path $rootDir "active_dll_path.txt"), $versionedDll, [System.Text.Encoding]::UTF8)
        Write-Host "        [註冊表熱切換] HKCU InprocServer32 -> $versionedDll" -ForegroundColor Cyan
    }

    # 同步單一真實詞庫來源 (ruhi_lexicon\data) 至專案內所有執行期與測試用 data 目錄
    $canonicalDataDir = Join-Path $rootDir "ruhi_lexicon\data"
    $workspaceDataTargets = @(
        (Join-Path $rootDir "data"),
        (Join-Path $rootDir "scripts\data"),
        (Join-Path $rootDir "dist\data"),
        (Join-Path $rootDir "dist\RuhI_v0.3.0_Windows_x64\data")
    )
    if (Test-Path $canonicalDataDir) {
        foreach ($wTarget in $workspaceDataTargets) {
            if (Test-Path $wTarget) {
                Get-ChildItem -Path $canonicalDataDir -File -Include "*.tsv", "*.jsonl", "*.json" -ErrorAction SilentlyContinue | ForEach-Object {
                    $destWFile = Join-Path $wTarget $_.Name
                    if ((-not (Test-Path $destWFile)) -or ($_.LastWriteTime -gt (Get-Item $destWFile).LastWriteTime)) {
                        Copy-Item $_.FullName -Destination $destWFile -Force -ErrorAction SilentlyContinue
                    }
                }
            }
        }
    }
} finally {
    Start-Process "ctfmon.exe"
    Start-Sleep -Milliseconds 150

    # 透過 TSF EnableLanguageProfile 雙向廣播與進程內橋接，使所有執行中視窗立即切換至新版 DLL 並刷新右下角狀態圖示
    try {
        Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

[ComImport, Guid("1F02B6C5-7842-4EE6-8A0B-9A24183A95CA"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface ITfInputProcessorProfiles {
    [PreserveSig] int Register([In] ref Guid rclsid);
    [PreserveSig] int Unregister([In] ref Guid rclsid);
    [PreserveSig] int AddLanguageProfile([In] ref Guid rclsid, ushort langid, [In] ref Guid guidProfile, [MarshalAs(UnmanagedType.LPWStr)] string pchDesc, uint cchDesc, [MarshalAs(UnmanagedType.LPWStr)] string pchIconFile, uint cchFile, uint uIconIndex);
    [PreserveSig] int RemoveLanguageProfile([In] ref Guid rclsid, ushort langid, [In] ref Guid guidProfile);
    [PreserveSig] int EnumInputProcessorInfo(out IntPtr ppEnum);
    [PreserveSig] int GetDefaultLanguageProfile(ushort langid, [In] ref Guid catid, out Guid pclsid, out Guid pguidProfile);
    [PreserveSig] int SetDefaultLanguageProfile(ushort langid, [In] ref Guid rclsid, [In] ref Guid guidProfile);
    [PreserveSig] int ActivateLanguageProfile([In] ref Guid rclsid, ushort langid, [In] ref Guid guidProfile);
    [PreserveSig] int GetActiveLanguageProfile([In] ref Guid rclsid, out ushort plangid, out Guid pguidProfile);
    [PreserveSig] int GetLanguageProfileDescription([In] ref Guid rclsid, ushort langid, [In] ref Guid guidProfile, out IntPtr pbstrProfile);
    [PreserveSig] int GetCurrentLanguage(out ushort plangid);
    [PreserveSig] int ChangeCurrentLanguage(ushort langid);
    [PreserveSig] int GetLanguageList(out IntPtr ppabLangId, out uint pulCount);
    [PreserveSig] int EnumLanguageProfiles(ushort langid, out IntPtr ppEnum);
    [PreserveSig] int EnableLanguageProfile([In] ref Guid rclsid, ushort langid, [In] ref Guid guidProfile, int fEnable);
}

public class RuhiLiveBridge {
    [DllImport("msctf.dll")]
    public static extern int TF_CreateInputProcessorProfiles(out ITfInputProcessorProfiles ppipr);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr OpenProcess(uint dwDesiredAccess, bool bInheritHandle, int dwProcessId);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr VirtualAllocEx(IntPtr hProcess, IntPtr lpAddress, uint dwSize, uint flAllocationType, uint flProtect);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool WriteProcessMemory(IntPtr hProcess, IntPtr lpBaseAddress, byte[] lpBuffer, uint nSize, out IntPtr lpNumberOfBytesWritten);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool VirtualProtectEx(IntPtr hProcess, IntPtr lpAddress, uint dwSize, uint flNewProtect, out uint lpflOldProtect);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr CreateRemoteThread(IntPtr hProcess, IntPtr lpThreadAttributes, uint dwStackSize, IntPtr lpStartAddress, IntPtr lpParameter, uint dwCreationFlags, out uint lpThreadId);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern uint WaitForSingleObject(IntPtr hHandle, uint dwMilliseconds);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool CloseHandle(IntPtr hObject);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr GetModuleHandleW(string lpModuleName);

    [DllImport("kernel32.dll", CharSet = CharSet.Ansi)]
    public static extern IntPtr GetProcAddress(IntPtr hModule, string lpProcName);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr LoadLibraryW(string lpLibFileName);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr FindWindowExW(IntPtr hwndParent, IntPtr hwndChildAfter, string lpszClass, string lpszWindow);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct NOTIFYICONDATAW {
        public uint cbSize;
        public IntPtr hWnd;
        public uint uID;
        public uint uFlags;
        public uint uCallbackMessage;
        public IntPtr hIcon;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string szTip;
        public uint dwState;
        public uint dwStateMask;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)]
        public string szInfo;
        public uint uVersion;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)]
        public string szInfoTitle;
        public uint dwInfoFlags;
        public Guid guidItem;
        public IntPtr hBalloonIcon;
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    public static extern bool Shell_NotifyIconW(uint dwMessage, ref NOTIFYICONDATAW lpData);

    public static void PurgeLegacyTrayIcons() {
        IntPtr child = IntPtr.Zero;
        for (int i = 0; i < 64; i++) {
            child = FindWindowExW(IntPtr.Zero, child, "RuhICandidateWindow", null);
            if (child == IntPtr.Zero) break;
            NOTIFYICONDATAW nid = new NOTIFYICONDATAW();
            nid.cbSize = (uint)Marshal.SizeOf(typeof(NOTIFYICONDATAW));
            nid.hWnd = child;
            nid.uID = 1001;
            Shell_NotifyIconW(0x00000002, ref nid);
        }
    }

    public static bool BridgeOldModuleInProcess(int pid, IntPtr oldBase, int oldExportOffset, string newDllPath, int newExportOffset) {
        IntPtr hProc = OpenProcess(0x0002 | 0x0008 | 0x0010 | 0x0020 | 0x0400, false, pid);
        if (hProc == IntPtr.Zero) return false;
        try {
            IntPtr hKernel32 = GetModuleHandleW("kernel32.dll");
            IntPtr pLoadLib = GetProcAddress(hKernel32, "LoadLibraryW");
            byte[] pathBytes = System.Text.Encoding.Unicode.GetBytes(newDllPath + "\0");
            IntPtr remoteMem = VirtualAllocEx(hProc, IntPtr.Zero, (uint)pathBytes.Length, 0x1000 | 0x2000, 0x04);
            if (remoteMem == IntPtr.Zero) return false;
            IntPtr written;
            if (!WriteProcessMemory(hProc, remoteMem, pathBytes, (uint)pathBytes.Length, out written)) return false;
            uint tid;
            IntPtr hThread = CreateRemoteThread(hProc, IntPtr.Zero, 0, pLoadLib, remoteMem, 0, out tid);
            if (hThread == IntPtr.Zero) return false;
            WaitForSingleObject(hThread, 3000);
            CloseHandle(hThread);
            return true;
        } finally {
            CloseHandle(hProc);
        }
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool ReadProcessMemory(IntPtr hProcess, IntPtr lpBaseAddress, byte[] lpBuffer, uint nSize, out IntPtr lpNumberOfBytesRead);

    public static IntPtr ResolveRemoteExport(IntPtr hProc, IntPtr modBase, string targetName) {
        byte[] hdr = new byte[1024];
        IntPtr read;
        if (!ReadProcessMemory(hProc, modBase, hdr, 1024, out read)) return IntPtr.Zero;
        if (hdr[0] != 0x4D || hdr[1] != 0x5A) return IntPtr.Zero;
        int e_lfanew = BitConverter.ToInt32(hdr, 0x3C);
        if (e_lfanew <= 0 || e_lfanew + 0x90 > 1024) return IntPtr.Zero;
        int exportRva = BitConverter.ToInt32(hdr, e_lfanew + 0x88);
        if (exportRva <= 0) return IntPtr.Zero;
        byte[] expDir = new byte[40];
        if (!ReadProcessMemory(hProc, (IntPtr)(modBase.ToInt64() + exportRva), expDir, 40, out read)) return IntPtr.Zero;
        int numNames = BitConverter.ToInt32(expDir, 24);
        int addrFuncsRva = BitConverter.ToInt32(expDir, 28);
        int addrNamesRva = BitConverter.ToInt32(expDir, 32);
        int addrOrdsRva = BitConverter.ToInt32(expDir, 36);
        if (numNames <= 0 || numNames > 256) return IntPtr.Zero;
        byte[] namesBuf = new byte[numNames * 4];
        byte[] ordsBuf = new byte[numNames * 2];
        if (!ReadProcessMemory(hProc, (IntPtr)(modBase.ToInt64() + addrNamesRva), namesBuf, (uint)namesBuf.Length, out read)) return IntPtr.Zero;
        if (!ReadProcessMemory(hProc, (IntPtr)(modBase.ToInt64() + addrOrdsRva), ordsBuf, (uint)ordsBuf.Length, out read)) return IntPtr.Zero;
        byte[] nameBytes = new byte[64];
        for (int i = 0; i < numNames; i++) {
            int nameRva = BitConverter.ToInt32(namesBuf, i * 4);
            if (!ReadProcessMemory(hProc, (IntPtr)(modBase.ToInt64() + nameRva), nameBytes, 64, out read)) continue;
            int len = Array.IndexOf(nameBytes, (byte)0);
            if (len < 0) len = 64;
            string s = System.Text.Encoding.ASCII.GetString(nameBytes, 0, len);
            if (s == targetName) {
                ushort ord = BitConverter.ToUInt16(ordsBuf, i * 2);
                byte[] funcRvaBuf = new byte[4];
                if (!ReadProcessMemory(hProc, (IntPtr)(modBase.ToInt64() + addrFuncsRva + ord * 4), funcRvaBuf, 4, out read)) return IntPtr.Zero;
                int funcRva = BitConverter.ToInt32(funcRvaBuf, 0);
                return (IntPtr)(modBase.ToInt64() + funcRva);
            }
        }
        return IntPtr.Zero;
    }

    public static int FindLbiVtableRva(IntPtr hProc, IntPtr modBase) {
        byte[] hdr = new byte[1024];
        IntPtr read;
        if (!ReadProcessMemory(hProc, modBase, hdr, 1024, out read)) return 0;
        if (hdr[0] != 0x4D || hdr[1] != 0x5A) return 0;
        int e_lfanew = BitConverter.ToInt32(hdr, 0x3C);
        if (e_lfanew <= 0 || e_lfanew + 0x108 > 1024) return 0;
        ushort numSecs = BitConverter.ToUInt16(hdr, e_lfanew + 6);
        ushort optSize = BitConverter.ToUInt16(hdr, e_lfanew + 20);
        int secOff = e_lfanew + 24 + optSize;
        int textLo = 0, textHi = 0, rdataRva = 0, rdataSize = 0;
        for (int i = 0; i < numSecs; i++) {
            int o = secOff + i * 40;
            if (o + 40 > 1024) break;
            string name = System.Text.Encoding.ASCII.GetString(hdr, o, 8).TrimEnd('\0');
            int vsize = BitConverter.ToInt32(hdr, o + 8);
            int rva = BitConverter.ToInt32(hdr, o + 12);
            if (name == ".text") { textLo = rva; textHi = rva + vsize; }
            else if (name == ".rdata") { rdataRva = rva; rdataSize = vsize; }
        }
        if (textLo <= 0 || rdataRva <= 0 || rdataSize <= 17 * 8 || rdataSize > 0x400000) return 0;
        byte[] rdata = new byte[rdataSize];
        if (!ReadProcessMemory(hProc, (IntPtr)(modBase.ToInt64() + rdataRva), rdata, (uint)rdataSize, out read)) return 0;
        ulong baseAddr = (ulong)modBase.ToInt64();
        ulong uTextLo = baseAddr + (ulong)textLo;
        ulong uTextHi = baseAddr + (ulong)textHi;
        int limit = rdataSize - 17 * 8;
        int[] unpatched = new int[] { 0, 1, 2, 5, 12, 13, 14, 15, 16 };
        byte[] fnBuf = new byte[16];
        for (int off = 0; off <= limit; off += 8) {
            bool allInText = true;
            foreach (int k in unpatched) {
                ulong pk = BitConverter.ToUInt64(rdata, off + k * 8);
                if (pk < uTextLo || pk >= uTextHi) { allInText = false; break; }
            }
            if (!allInText) continue;
            ulong p5 = BitConverter.ToUInt64(rdata, off + 5 * 8);
            if (!ReadProcessMemory(hProc, (IntPtr)p5, fnBuf, 16, out read)) continue;
            if (fnBuf[0] == 0x48 && fnBuf[1] == 0x83 && fnBuf[2] == 0xEC && fnBuf[3] == 0x28 && fnBuf[4] == 0xE8 &&
                fnBuf[9] == 0x31 && fnBuf[10] == 0xC0 && fnBuf[11] == 0x48 && fnBuf[12] == 0x83 && fnBuf[13] == 0xC4 && fnBuf[14] == 0x28 && fnBuf[15] == 0xC3) {
                return rdataRva + off;
            }
        }
        return 0;
    }

    public static int FindVtableBlockRva(IntPtr hProc, IntPtr modBase) {
        byte[] hdr = new byte[1024];
        IntPtr read;
        if (!ReadProcessMemory(hProc, modBase, hdr, 1024, out read)) return 0;
        if (hdr[0] != 0x4D || hdr[1] != 0x5A) return 0;
        int e_lfanew = BitConverter.ToInt32(hdr, 0x3C);
        if (e_lfanew <= 0 || e_lfanew + 0x108 > 1024) return 0;
        ushort numSecs = BitConverter.ToUInt16(hdr, e_lfanew + 6);
        ushort optSize = BitConverter.ToUInt16(hdr, e_lfanew + 20);
        int secOff = e_lfanew + 24 + optSize;
        int textLo = 0, textHi = 0, rdataRva = 0, rdataSize = 0;
        for (int i = 0; i < numSecs; i++) {
            int o = secOff + i * 40;
            if (o + 40 > 1024) break;
            string name = System.Text.Encoding.ASCII.GetString(hdr, o, 8).TrimEnd('\0');
            int vsize = BitConverter.ToInt32(hdr, o + 8);
            int rva = BitConverter.ToInt32(hdr, o + 12);
            if (name == ".text") { textLo = rva; textHi = rva + vsize; }
            else if (name == ".rdata") { rdataRva = rva; rdataSize = vsize; }
        }
        if (textLo <= 0 || rdataRva <= 0 || rdataSize <= 37 * 8 || rdataSize > 0x400000) return 0;
        byte[] rdata = new byte[rdataSize];
        if (!ReadProcessMemory(hProc, (IntPtr)(modBase.ToInt64() + rdataRva), rdata, (uint)rdataSize, out read)) return 0;
        ulong baseAddr = (ulong)modBase.ToInt64();
        ulong uTextLo = baseAddr + (ulong)textLo;
        ulong uTextHi = baseAddr + (ulong)textHi;
        int limit = rdataSize - 37 * 8;
        int[] unpatchedSlots = new int[] { 0, 1, 2, 5, 6, 7, 11, 12, 13, 20, 21, 22, 23, 24, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36 };
        for (int off = 0; off <= limit; off += 8) {
            ulong p23 = BitConverter.ToUInt64(rdata, off + 23 * 8);
            if (p23 < uTextLo || p23 >= uTextHi) continue;
            if (p23 != BitConverter.ToUInt64(rdata, off + 24 * 8)) continue;
            if (p23 != BitConverter.ToUInt64(rdata, off + 26 * 8)) continue;
            if (p23 != BitConverter.ToUInt64(rdata, off + 27 * 8)) continue;
            if (p23 == BitConverter.ToUInt64(rdata, off + 25 * 8)) continue;
            bool allInText = true;
            foreach (int k in unpatchedSlots) {
                ulong pk = BitConverter.ToUInt64(rdata, off + k * 8);
                if (pk < uTextLo || pk >= uTextHi) { allInText = false; break; }
            }
            if (allInText) return rdataRva + off;
        }
        return 0;
    }

    public static bool PatchModuleDllGetClassObject(int pid, IntPtr oldModBase, IntPtr newExportAddr) {
        IntPtr hProc = OpenProcess(0x0008 | 0x0010 | 0x0020, false, pid);
        if (hProc == IntPtr.Zero) return false;
        try {
            IntPtr oldExportAddr = ResolveRemoteExport(hProc, oldModBase, "DllGetClassObject");
            if (oldExportAddr == IntPtr.Zero) return false;
            uint oldProt;
            if (!VirtualProtectEx(hProc, oldExportAddr, 16, 0x40, out oldProt)) return false;
            byte[] patch = new byte[12];
            patch[0] = 0x48;
            patch[1] = 0xB8;
            byte[] addrBytes = BitConverter.GetBytes(newExportAddr.ToInt64());
            Array.Copy(addrBytes, 0, patch, 2, 8);
            patch[10] = 0xFF;
            patch[11] = 0xE0;
            IntPtr written;
            bool ok = WriteProcessMemory(hProc, oldExportAddr, patch, 12, out written);
            uint dummy;
            VirtualProtectEx(hProc, oldExportAddr, 16, oldProt, out dummy);
            return ok;
        } finally {
            CloseHandle(hProc);
        }
    }

    public static bool PatchModuleVtables(int pid, IntPtr oldModBase, IntPtr newModBase) {
        IntPtr hProc = OpenProcess(0x0008 | 0x0010 | 0x0020, false, pid);
        if (hProc == IntPtr.Zero) return false;
        try {
            int oldVtblRva = FindVtableBlockRva(hProc, oldModBase);
            int newVtblRva = FindVtableBlockRva(hProc, newModBase);
            if (oldVtblRva <= 0x208 || newVtblRva <= 0x208) return false;
            IntPtr read, written;
            uint oldProt, dummy;

            // 1. Patch RuhiLangBarItemButton vtable (17-pointer block located by FindLbiVtableRva)
            int oldLbiRva = FindLbiVtableRva(hProc, oldModBase);
            int newLbiRva = FindLbiVtableRva(hProc, newModBase);
            if (oldLbiRva > 0 && newLbiRva > 0) {
                IntPtr oldLbiAddr = (IntPtr)(oldModBase.ToInt64() + oldLbiRva);
                IntPtr newLbiAddr = (IntPtr)(newModBase.ToInt64() + newLbiRva);
                byte[] oldLbiBuf = new byte[17 * 8];
                byte[] newLbiBuf = new byte[17 * 8];
                if (ReadProcessMemory(hProc, oldLbiAddr, oldLbiBuf, (uint)oldLbiBuf.Length, out read) &&
                    ReadProcessMemory(hProc, newLbiAddr, newLbiBuf, (uint)newLbiBuf.Length, out read)) {
                    int[] lbiSlots = new int[] { 3, 4, 6, 7, 8, 9, 10, 11 };
                    foreach (int s in lbiSlots) {
                        Array.Copy(newLbiBuf, s * 8, oldLbiBuf, s * 8, 8);
                    }
                    if (VirtualProtectEx(hProc, oldLbiAddr, (uint)oldLbiBuf.Length, 0x04, out oldProt)) {
                        WriteProcessMemory(hProc, oldLbiAddr, oldLbiBuf, (uint)oldLbiBuf.Length, out written);
                        VirtualProtectEx(hProc, oldLbiAddr, (uint)oldLbiBuf.Length, oldProt, out dummy);
                    }
                }
            }

            // 2. Patch RuhiTextService vtable (at ts_rva)
            IntPtr oldVtblAddr = (IntPtr)(oldModBase.ToInt64() + oldVtblRva);
            IntPtr newVtblAddr = (IntPtr)(newModBase.ToInt64() + newVtblRva);
            byte[] oldBuf = new byte[37 * 8];
            byte[] newBuf = new byte[37 * 8];
            if (!ReadProcessMemory(hProc, oldVtblAddr, oldBuf, (uint)oldBuf.Length, out read)) return false;
            if (!ReadProcessMemory(hProc, newVtblAddr, newBuf, (uint)newBuf.Length, out read)) return false;
            int[] slotsToPatch = new int[] { 3, 4, 8, 9, 10, 14, 15, 16, 17, 18, 19, 25 };
            foreach (int s in slotsToPatch) {
                Array.Copy(newBuf, s * 8, oldBuf, s * 8, 8);
            }
            if (!VirtualProtectEx(hProc, oldVtblAddr, (uint)oldBuf.Length, 0x04, out oldProt)) return false;
            bool ok = WriteProcessMemory(hProc, oldVtblAddr, oldBuf, (uint)oldBuf.Length, out written);
            VirtualProtectEx(hProc, oldVtblAddr, (uint)oldBuf.Length, oldProt, out dummy);
            return ok;
        } finally {
            CloseHandle(hProc);
        }
    }

    public static void BroadcastReload() {
        ITfInputProcessorProfiles profiles;
        if (TF_CreateInputProcessorProfiles(out profiles) == 0 && profiles != null) {
            Guid clsid = new Guid("E0B7F8C1-1823-42D8-99B1-5E8A719D3F10");
            Guid profile = new Guid("A1D9C3F2-4761-4E5A-8C3D-1F2E3D4C5B6A");
            profiles.EnableLanguageProfile(ref clsid, 0x0404, ref profile, 0);
            System.Threading.Thread.Sleep(180);
            profiles.EnableLanguageProfile(ref clsid, 0x0404, ref profile, 1);
            profiles.ActivateLanguageProfile(ref clsid, 0x0404, ref profile);
        }
    }
}
"@ -ErrorAction SilentlyContinue

        [RuhiLiveBridge]::PurgeLegacyTrayIcons()

        if (Test-Path $versionedDll) {
            $hLocalNew = [RuhiLiveBridge]::LoadLibraryW($versionedDll)
            $pLocalGetClass = [RuhiLiveBridge]::GetProcAddress($hLocalNew, "DllGetClassObject")
            $newExportRva = [int]($pLocalGetClass.ToInt64() - $hLocalNew.ToInt64())

            $curSession = (Get-Process -Id $PID).SessionId
            $versionedLeaf = Split-Path -Leaf $versionedDll
            Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $curSession -and $_.Id -ne $PID } | ForEach-Object {
                $proc = $_
                try {
                    $oldMods = @($proc.Modules | Where-Object { $_.ModuleName -like "ruhi_tsf*.dll" -and $_.ModuleName -ne $versionedLeaf })
                    if ($oldMods.Count -gt 0) {
                        [RuhiLiveBridge]::BridgeOldModuleInProcess($proc.Id, $oldMods[0].BaseAddress, 0, $versionedDll, $newExportRva) | Out-Null
                        $proc.Refresh()
                        $newMod = $proc.Modules | Where-Object { $_.ModuleName -eq $versionedLeaf } | Select-Object -First 1
                        if ($newMod) {
                            $newAddr = [IntPtr]($newMod.BaseAddress.ToInt64() + $newExportRva)
                            foreach ($om in $oldMods) {
                                [RuhiLiveBridge]::PatchModuleDllGetClassObject($proc.Id, $om.BaseAddress, $newAddr) | Out-Null
                                [RuhiLiveBridge]::PatchModuleVtables($proc.Id, $om.BaseAddress, $newMod.BaseAddress) | Out-Null
                            }
                        }
                    }
                } catch {}
            }
        }

        [RuhiLiveBridge]::BroadcastReload()
        [RuhiLiveBridge]::PurgeLegacyTrayIcons()
    } catch {}

    Write-Host "  [4/4] 已安全重啟文字服務框架監聽 (ctfmon) 並完成全進程無感熱切換！" -ForegroundColor Green
}

Write-Host ""
Write-Host "========================================================" -ForegroundColor Cyan
if ($updatedCount -gt 0) {
    Write-Host "  【大功告成！】最新版本 DLL 與完整詞庫已成功熱替換完成！" -ForegroundColor Green
    Write-Host "  ★ 已啟用 InprocServer32 版本化熱加載，所有現有視窗免重啟即可生效！" -ForegroundColor Yellow
} else {
    Write-Host "  【提示】未更新任何目標，請檢查權限或建置輸出。" -ForegroundColor Yellow
}
Write-Host "========================================================" -ForegroundColor Cyan
