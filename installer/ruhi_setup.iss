; RuhI (如喜) 通用多語系智慧型輸入法 - 官方 Inno Setup 標準安裝腳本
; 符合 Windows 10 / 11 64 位元純 TSF 與 Win32 全生態相容性規範 (TSF_WIN32_COMPATIBILITY_AND_INSTALLER_SPEC)

#define MyAppName "RuhI 如喜通用智慧型輸入法"
#define MyAppVersion "0.2.0"
#define MyAppPublisher "RuhOS Community"
#define MyAppURL "https://github.com/RuhOS/ruhi"
#define MyAppExeName "run_settings.ps1"

[Setup]
AppId={{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\RuhI
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
OutputDir=..\dist
OutputBaseFilename=RuhI_Setup_v{#MyAppVersion}
SetupIconFile=
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
CloseApplications=force

[Languages]
Name: "chinesetraditional"; MessagesFile: "compiler:Languages\ChineseTraditional.isl"

[Files]
; 核心二進位檔
Source: "..\dist\ruhi_tsf.dll"; DestDir: "{app}"; Flags: ignoreversion restartreplace uninsrestartdelete
Source: "..\dist\ruhi.dll"; DestDir: "{app}"; Flags: ignoreversion restartreplace uninsrestartdelete

; 完整詞庫與轉移引力資料
Source: "..\dist\data\*"; DestDir: "{app}\data"; Flags: ignoreversion recursesubdirs createallsubdirs

; 偏好設定中心與設定檔
Source: "..\dist\settings_window.py"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\settings_config.json"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\config.json"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\user_dict.tsv"; DestDir: "{app}"; Flags: ignoreversion onlyifdoesntexist

; 管理與日常熱重載工具
Source: "..\dist\run_settings.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\hot_reload.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\一鍵安裝.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\一鍵解除安裝.ps1"; DestDir: "{app}"; Flags: ignoreversion

; 專案宣告與文件
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\如喜輸入法偏好設定"; Filename: "powershell.exe"; Parameters: "-ExecutionPolicy Bypass -File ""{app}\run_settings.ps1"""; WorkingDir: "{app}"
Name: "{group}\如喜輸入法熱更新工具"; Filename: "powershell.exe"; Parameters: "-ExecutionPolicy Bypass -File ""{app}\hot_reload.ps1"""; WorkingDir: "{app}"
Name: "{group}\解除安裝如喜輸入法"; Filename: "{uninstallexe}"

[Code]
// 執行外部指令輔助函式
procedure ExecCmd(const Cmd: string; const Params: string);
var
  ResultCode: Integer;
begin
  Exec(Cmd, Params, '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

// 暫停文字服務框架
procedure StopCtfmon();
begin
  ExecCmd('taskkill.exe', '/f /im ctfmon.exe');
  Sleep(300);
end;

// 喚起文字服務框架
procedure StartCtfmon();
var
  ResultCode: Integer;
begin
  Exec('ctfmon.exe', '', '', SW_SHOWNORMAL, ewNoWait, ResultCode);
end;

// 安裝前準備
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopCtfmon();
  Result := '';
end;

// 安裝後步驟：落實 TSF_WIN32_COMPATIBILITY_AND_INSTALLER_SPEC 核心配置
procedure CurStepChanged(CurStep: TSetupStep);
var
  DllPath: string;
  AppDir: string;
begin
  if CurStep = ssPostInstall then
  begin
    AppDir := ExpandConstant('{app}');
    DllPath := ExpandConstant('{app}\ruhi_tsf.dll');

    // 1. 配置安全性存取清單 (ACL: ALL_APPLICATION_PACKAGES 及 Users)
    ExecCmd('icacls.exe', '"' + AppDir + '" /grant *S-1-15-2-1:(OI)(CI)RX /T /Q');
    ExecCmd('icacls.exe', '"' + AppDir + '" /grant Users:(OI)(CI)RX /T /Q');

    // 2. 註冊 TSF COM 伺服器
    ExecCmd('regsvr32.exe', '/s "' + DllPath + '"');

    // 3. 清理無效 SubstituteLayout (純 TSF TIP 絕不包含無效實體鍵盤代碼)
    RegDeleteValue(HKEY_LOCAL_MACHINE,
      'SOFTWARE\Microsoft\CTF\TIP\{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}\LanguageProfile\0x00000404\{F52A781B-8321-48E6-A145-C89B0981E7D2}',
      'SubstituteLayout');

    // 4. 配置 HKCU Assemblies 與 SortOrder 為 0x04040404 (確保傳統 Win32 軟體如 LibreOffice / 命令提示字元即裝即用)
    RegWriteStringValue(HKEY_CURRENT_USER,
      'Software\Microsoft\CTF\Assemblies\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}',
      'Default', '{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}');
    RegWriteStringValue(HKEY_CURRENT_USER,
      'Software\Microsoft\CTF\Assemblies\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}',
      'Profile', '{F52A781B-8321-48E6-A145-C89B0981E7D2}');
    RegWriteDWordValue(HKEY_CURRENT_USER,
      'Software\Microsoft\CTF\Assemblies\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}',
      'KeyboardLayout', $04040404);

    RegWriteStringValue(HKEY_CURRENT_USER,
      'Software\Microsoft\CTF\SortOrder\AssemblyItem\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}\00000000',
      'CLSID', '{E0B7F8C1-1823-42D8-99B1-5E8A719D3F10}');
    RegWriteStringValue(HKEY_CURRENT_USER,
      'Software\Microsoft\CTF\SortOrder\AssemblyItem\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}\00000000',
      'Profile', '{F52A781B-8321-48E6-A145-C89B0981E7D2}');
    RegWriteDWordValue(HKEY_CURRENT_USER,
      'Software\Microsoft\CTF\SortOrder\AssemblyItem\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}\00000000',
      'KeyboardLayout', $04040404);

    // 5. 重新喚起文字服務框架
    StartCtfmon();
  end;
end;

// 解除安裝流程
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DllPath: string;
begin
  if CurUninstallStep = usUninstall then
  begin
    StopCtfmon();
    DllPath := ExpandConstant('{app}\ruhi_tsf.dll');

    // 反註冊 COM 伺服器
    ExecCmd('regsvr32.exe', '/u /s "' + DllPath + '"');

    // 清理 Assemblies 與 SortOrder 鍵值
    RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER,
      'Software\Microsoft\CTF\Assemblies\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}');
    RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER,
      'Software\Microsoft\CTF\SortOrder\AssemblyItem\0x00000404\{34745C63-B2F0-4784-8B67-5E12C8701A31}');

    StartCtfmon();
  end;
end;
