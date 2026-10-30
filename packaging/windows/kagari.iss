; The Windows installer of the Kagari toolchain (Inno Setup 6 or 7). The release workflow
; (.github/workflows/release.yml) builds it from the release executables:
;
;   ISCC.exe /DAppVersion=0.1.0 /DNumericVersion=0.1.0 /DSourceDir=<dir> /DOutputDir=<dir> kagari.iss
;
; SourceDir holds kagari.exe, kagari-desktop-runtime.exe and LICENSE. The installer is per user:
; it needs no administrator rights, installs into %LOCALAPPDATA%\Programs\Kagari and adds its
; bin directory to the user's PATH. Installing another version replaces the installed one.

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

[Files]
Source: "{#SourceDir}\kagari.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#SourceDir}\kagari-desktop-runtime.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; \
    ValueData: "{olddata};{app}\bin"; Check: NeedsAddPath(ExpandConstant('{app}\bin'))

[Code]
const
  EnvironmentKey = 'Environment';

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
