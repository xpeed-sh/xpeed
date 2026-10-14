#ifndef AppVersion
  #error AppVersion must be provided by scripts/package-installer.ps1
#endif
#ifndef ProjectRoot
  #error ProjectRoot must be provided by scripts/package-installer.ps1
#endif
#define Payload ProjectRoot + "\dist\portable\Xpeed-" + AppVersion
#ifndef InstallId
  #define InstallId "xpeed.desktop.D420A53A-899E-43D2-90B3-C57499E3ACFE"
#endif

[Setup]
AppId={#InstallId}
AppName=Xpeed
AppVersion={#AppVersion}
AppPublisher=Ronald Ramos
AppPublisherURL=https://xpeed.ronaldbit.com
DefaultDirName={localappdata}\Programs\Xpeed
DefaultGroupName=Xpeed
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#ProjectRoot}\dist\installer
OutputBaseFilename=Xpeed-Setup-{#AppVersion}-x64
SetupIconFile={#ProjectRoot}\assets\img\xpeed.ico
UninstallDisplayIcon={app}\Xpeed.exe
LicenseFile={#ProjectRoot}\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
SetupLogging=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Payload}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Xpeed"; Filename: "{app}\Xpeed.exe"; WorkingDir: "{app}"; AppUserModelID: "xpeed.desktop"
Name: "{autodesktop}\Xpeed"; Filename: "{app}\Xpeed.exe"; WorkingDir: "{app}"; Tasks: desktopicon; AppUserModelID: "xpeed.desktop"

[Run]
Filename: "{app}\Xpeed.exe"; Description: "{cm:LaunchProgram,Xpeed}"; Flags: nowait postinstall skipifsilent

; Personal data in AppData\Roaming\xpeed is never shipped or deleted.
