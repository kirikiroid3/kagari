; The Windows installer of the Kagari toolchain (Inno Setup 6 or 7). The release workflow
; (.github/workflows/release.yml) builds it from the release executables:
;
;   ISCC.exe /DAppVersion=0.1.0 /DNumericVersion=0.1.0 /DSourceDir=<dir> /DOutputDir=<dir> kagari.iss
;
; SourceDir holds kagari.exe, kagari-desktop-runtime.exe, LICENSE and vc_redist.x64.exe (the
; Visual C++ Redistributable of the Visual Studio that linked the executables). The installer is
; per user: it installs into %LOCALAPPDATA%\Programs\Kagari and adds its bin directory to the
; user's PATH. Installing another version replaces the installed one. The executables need the
; MSVC C runtime (vcruntime140.dll): when no Redistributable at least as new as the bundled one is
; registered, the installer runs the bundled one first, which asks for administrator rights.

#ifndef AppVersion
  #error Define AppVersion, e.g. /DAppVersion=0.1.0
#endif
#ifndef NumericVersion
  #error Define NumericVersion (digits and dots only), e.g. /DNumericVersion=0.1.0
#endif
#ifndef SourceDir
  #error Define SourceDir, the directory with the executables and LICENSE
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

#define VCRedistVersion GetVersionNumbersString(AddBackslash(SourceDir) + "vc_redist.x64.exe")
#if VCRedistVersion == "" || VCRedistVersion == "0.0.0.0"
  #error SourceDir lacks vc_redist.x64.exe or its file version
#endif

[Setup]
; Never change AppId: it identifies the installation across versions.
AppId={{ECBFA02F-137F-4784-B8F0-7C52CA183EFF}
AppName=Kagari
AppVersion={#AppVersion}
AppVerName=Kagari {#AppVersion}
AppPublisher=Nikaidou Haruki
AppPublisherURL=https://kagari.suitsu31.club/
AppSupportURL=https://github.com/kirikiroid3/kagari
VersionInfoVersion={#NumericVersion}
DefaultDirName={autopf}\Kagari
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesEnvironment=yes
OutputDir={#OutputDir}
OutputBaseFilename=kagari-{#AppVersion}-x86_64-windows-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName=Kagari

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ja"; MessagesFile: "compiler:Languages\Japanese.isl"

[CustomMessages]
en.VCRedistFailed=Kagari needs the Microsoft Visual C++ Redistributable (x64), which could not be installed (%1). Install it from https://aka.ms/vs/17/release/vc_redist.x64.exe, then run this installer again.
ja.VCRedistFailed=Kagari には Microsoft Visual C++ 再頒布可能パッケージ (x64) が必要ですが、インストールできませんでした（%1）。https://aka.ms/vs/17/release/vc_redist.x64.exe からインストールしてから、このインストーラーをもう一度実行してください。

[Files]
Source: "{#SourceDir}\kagari.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#SourceDir}\kagari-desktop-runtime.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\vc_redist.x64.exe"; Flags: dontcopy

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; \
    ValueData: "{olddata};{app}\bin"; Check: NeedsAddPath(ExpandConstant('{app}\bin'))

[Code]
const
  EnvironmentKey = 'Environment';
  // Where the Visual C++ 2015 and later Redistributable (x64) registers its version.
  VCRuntimeKey = 'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64';

var
  VCRedistRestart: Boolean;

// The version of the MSVC runtime registered in the RootKey view, packed; 0 when there is none.
function RegisteredVCRuntime(RootKey: Integer): Int64;
var
  Installed, Major, Minor, Bld, Rbld: Cardinal;
begin
  Result := 0;
  if RegQueryDWordValue(RootKey, VCRuntimeKey, 'Installed', Installed) and (Installed = 1)
    and RegQueryDWordValue(RootKey, VCRuntimeKey, 'Major', Major)
    and RegQueryDWordValue(RootKey, VCRuntimeKey, 'Minor', Minor)
    and RegQueryDWordValue(RootKey, VCRuntimeKey, 'Bld', Bld)
    and RegQueryDWordValue(RootKey, VCRuntimeKey, 'Rbld', Rbld) then
    Result := PackVersionComponents(Major, Minor, Bld, Rbld);
end;

// Whether neither registry view holds an MSVC runtime at least as new as the bundled one. A
// bundled version that does not parse counts as newer: running the Redistributable is harmless.
function NeedsVCRedist(): Boolean;
var
  Bundled: Int64;
begin
  if not StrToVersion('{#VCRedistVersion}', Bundled) then
    Result := True
  else
    Result := (ComparePackedVersion(RegisteredVCRuntime(HKLM64), Bundled) < 0)
      and (ComparePackedVersion(RegisteredVCRuntime(HKLM32), Bundled) < 0);
end;

// Installs the bundled Redistributable when needed, before any file is copied; an error stops
// the installation. 1638: a newer version is installed; 3010: it needs a restart.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  Result := '';
  if not NeedsVCRedist() then
    exit;
  ExtractTemporaryFile('vc_redist.x64.exe');
  if not ShellExec('', ExpandConstant('{tmp}\vc_redist.x64.exe'), '/install /passive /norestart',
      '', SW_SHOWNORMAL, ewWaitUntilTerminated, ResultCode) then
    Result := FmtMessage(CustomMessage('VCRedistFailed'), [SysErrorMessage(ResultCode)])
  else if ResultCode = 3010 then
    VCRedistRestart := True
  else if (ResultCode <> 0) and (ResultCode <> 1638) then
    Result := FmtMessage(CustomMessage('VCRedistFailed'), [IntToStr(ResultCode)]);
end;

function NeedRestart(): Boolean;
begin
  Result := VCRedistRestart;
end;

// Whether the user's PATH lacks Dir (compared case-insensitively, entry by entry).
function NeedsAddPath(Dir: string): Boolean;
var
  Path: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path) then
    Path := '';
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Path) + ';') = 0;
end;

// Removes the entry Dir from the user's PATH, keeping every other entry as it was.
procedure RemovePath(Dir: string);
var
  Path, Padded: string;
  Index: Integer;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path) then
    exit;
  Padded := ';' + Path + ';';
  Index := Pos(';' + Uppercase(Dir) + ';', Uppercase(Padded));
  if Index = 0 then
    exit;
  Delete(Padded, Index, Length(Dir) + 1);
  Path := Copy(Padded, 2, Length(Padded) - 2);
  RegWriteExpandStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', Path);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemovePath(ExpandConstant('{app}\bin'));
end;
