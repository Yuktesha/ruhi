; RuhI (如喜) 通用多語系智慧型輸入法 - 官方 Inno Setup 標準安裝腳本
; 符合 Windows 10 / 11 64 位元純 TSF 與 Win32 全生態相容性規範 (TSF_WIN32_COMPATIBILITY_AND_INSTALLER_SPEC)

#define MyAppName "RuhI 如喜通用智慧型輸入法"
#define MyAppVersion "0.5.5"
#define MyAppPublisher "RuhOS Community"
#define MyAppURL "https://github.com/Yuktesha/ruhi"
#define MyAppExeName "ruhi-studio.exe"

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
SetupIconFile=..\assets\ruhi.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
CloseApplications=force

[Languages]
Name: "chinesetraditional"; MessagesFile: "ChineseTraditional.isl"

[Tasks]
Name: "telemetry_consent"; Description: "同意在手動改字時記錄匿名對照句，並經啟發式個資過濾後依設定週期回傳至官方雲端語料表以協助優化語意（預設不勾選，隨時可於設定中心更改）"; Flags: unchecked

#define StageDir "..\dist\RuhI_v" + MyAppVersion + "_Windows_x64"

[Files]
; 核心二進位檔與圖示 (100% Pure Rust: ruhi_tsf.dll, ruhi.dll, ruhi-studio.exe)
Source: "{#StageDir}\ruhi_tsf.dll"; DestDir: "{app}"; Flags: ignoreversion restartreplace uninsrestartdelete
Source: "{#StageDir}\ruhi.dll"; DestDir: "{app}"; Flags: ignoreversion restartreplace uninsrestartdelete
Source: "{#StageDir}\ruhi-studio.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\ruhi.ico"; DestDir: "{app}"; Flags: ignoreversion

; 完整詞庫與轉移引力資料
Source: "{#StageDir}\data\*"; DestDir: "{app}\data"; Flags: ignoreversion recursesubdirs createallsubdirs

; 偏好設定中心設定檔與使用者詞庫（自不含個人資料與 Webhook 網址之範本建立；升級時完整保留使用者既有設定與自建詞庫）
Source: "{#StageDir}\settings_config.example.json"; DestDir: "{app}"; DestName: "settings_config.json"; Flags: ignoreversion onlyifdoesntexist
Source: "{#StageDir}\config.example.json"; DestDir: "{app}"; DestName: "config.json"; Flags: ignoreversion onlyifdoesntexist
Source: "{#StageDir}\settings_config.example.json"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\config.example.json"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\user_dict.tsv"; DestDir: "{app}"; Flags: ignoreversion onlyifdoesntexist

; 管理與日常熱重載工具
Source: "{#StageDir}\run_settings.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\hot_reload.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\一鍵安裝.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\一鍵解除安裝.ps1"; DestDir: "{app}"; Flags: ignoreversion

; 專案宣告與使用者文件
Source: "{#StageDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "{#StageDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\docs\RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "{#StageDir}\docs\RUHI_MULTILINGUAL_EXPANSION_GUIDE.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "{#StageDir}\docs\RUHI_MULTILINGUAL_EXPANSION_GUIDE_EN.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "{#StageDir}\docs\RUHI_WESTERN_INPUT_AND_MIXED_TYPING_BLUEPRINT.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "{#StageDir}\docs\RUHI_KOREAN_RR_HANDOVER.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "{#StageDir}\docs\TSF_WIN32_COMPATIBILITY_AND_INSTALLER_SPEC.md"; DestDir: "{app}\docs"; Flags: ignoreversion

[Icons]
Name: "{group}\如喜輸入法偏好設定"; Filename: "{app}\ruhi-studio.exe"; WorkingDir: "{app}"
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

    // 1. 配置最小權限安全性存取清單 (ACL)：
    //    - 程式主目錄與二進位檔 (.dll / .exe) 僅授予 ALL_APPLICATION_PACKAGES 與 Users 讀取與執行權限 (RX)，嚴防非管理員覆寫執行檔造成權限提升 (LPE)
    //    - 僅對 data\ 資料夾、設定檔 (config.json / settings_config.json) 與使用者詞庫 (user_dict.tsv) 授予 Users 修改權限 (M)
    ExecCmd('icacls.exe', '"' + AppDir + '" /grant *S-1-15-2-1:(OI)(CI)RX /T /Q');
    ExecCmd('icacls.exe', '"' + AppDir + '" /grant Users:(OI)(CI)RX /T /Q');
    ExecCmd('icacls.exe', '"' + AppDir + '\data" /grant Users:(OI)(CI)M /T /Q');
    ExecCmd('icacls.exe', '"' + AppDir + '\config.json" /grant Users:M /Q');
    ExecCmd('icacls.exe', '"' + AppDir + '\settings_config.json" /grant Users:M /Q');
    ExecCmd('icacls.exe', '"' + AppDir + '\user_dict.tsv" /grant Users:M /Q');

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

    // 5. 依據使用者安裝時勾選之同意狀態寫入 config.json (telemetry_consent)
    if WizardIsTaskSelected('telemetry_consent') then
      ExecCmd('powershell.exe', '-NoProfile -ExecutionPolicy Bypass -Command "$p=''' + AppDir + '\config.json''; if(Test-Path $p){$j=Get-Content $p -Raw -Encoding UTF8|ConvertFrom-Json; $j|Add-Member -NotePropertyName telemetry_consent -NotePropertyValue $true -Force; $j|ConvertTo-Json -Depth 5|Set-Content $p -Encoding UTF8}"')
    else
      ExecCmd('powershell.exe', '-NoProfile -ExecutionPolicy Bypass -Command "$p=''' + AppDir + '\config.json''; if(Test-Path $p){$j=Get-Content $p -Raw -Encoding UTF8|ConvertFrom-Json; $j|Add-Member -NotePropertyName telemetry_consent -NotePropertyValue $false -Force; $j|ConvertTo-Json -Depth 5|Set-Content $p -Encoding UTF8}"');

    // 6. 重新喚起文字服務框架
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
