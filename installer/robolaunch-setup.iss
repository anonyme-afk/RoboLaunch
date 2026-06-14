; ============================================================
;  RoboLaunch — Script Inno Setup 6
;  Génère : RoboLaunch-Setup-1.0.0.exe
;  Style : assistant classique + icônes + désinstalleur auto
; ============================================================

#define AppName        "RoboLaunch"
#define AppVersion     "1.0.0"
#define AppPublisher   "RoboLaunch"
#define AppURL         "https://robolaunch.app"
#define AppExeName     "robo-launch.exe"
#define AppDescription "Lance des agents IA connectés à Roblox Studio"

[Setup]
; Identifiants
AppId                    = {{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}
AppName                  = {#AppName}
AppVersion               = {#AppVersion}
AppPublisher             = {#AppPublisher}
AppPublisherURL          = {#AppURL}
AppSupportURL            = {#AppURL}
AppUpdatesURL            = {#AppURL}
AppComments              = {#AppDescription}

; Répertoire d'installation
DefaultDirName           = {autopf}\{#AppName}
DefaultGroupName         = {#AppName}
DisableProgramGroupPage  = yes

; Fichier de sortie
OutputDir                = output
OutputBaseFilename       = RoboLaunch-Setup-{#AppVersion}
SetupIconFile            = assets\icon.ico

; Compression max
Compression              = lzma2/ultra64
SolidCompression         = yes
LZMAUseSeparateProcess   = yes

; Style wizard
WizardStyle              = modern
WizardSizePercent        = 120
WizardImageFile          = assets\sidebar.bmp
WizardSmallImageFile     = assets\icon_small.bmp

; Sécurité & compatibilité
PrivilegesRequired       = admin
MinVersion               = 10.0.19041
ArchitecturesAllowed     = x64compatible
ArchitecturesInstallIn64BitMode = x64compatible

; Désinstalleur
UninstallDisplayIcon     = {app}\{#AppExeName}
UninstallDisplayName     = {#AppName} {#AppVersion}
CreateUninstallRegKey    = yes

; Divers
ShowLanguageDialog       = no
LanguageDetectionMethod  = locale

[Languages]
Name: "french";  MessagesFile: "compiler:Languages\French.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

; ── Pages de l'assistant ──────────────────────────────────
[CustomMessages]
french.WelcomeLabel1     = Bienvenue dans l'installation de [name]
french.WelcomeLabel2     = Cet assistant va installer [name/ver] sur votre ordinateur.%n%nChez [name], les agents IA (Claude, Codex, Agy, Aider) s'exécutent dans une VM Linux isolée et communiquent avec Roblox Studio via MCP.%n%nFermez toutes les autres applications avant de continuer.
french.FinishedLabel     = L'installation de [name] est terminée.%n%nCliquez sur Terminer pour fermer cet assistant.
english.WelcomeLabel2    = This wizard will install [name/ver] on your computer.%n%nClose all other applications before continuing.

; ── Fichiers à installer ──────────────────────────────────
[Files]
; Exécutable principal
Source: "..\src-tauri\target\release\{#AppExeName}"; \
        DestDir: "{app}"; \
        Flags: ignoreversion

; DLLs Rust/Tauri générées
Source: "..\src-tauri\target\release\*.dll"; \
        DestDir: "{app}"; \
        Flags: ignoreversion recursesubdirs

; Ressources WebView2 (Tauri)
Source: "..\src-tauri\target\release\WebView2Loader.dll"; \
        DestDir: "{app}"; \
        Flags: ignoreversion skipifsourcedoesntexist

; VM Linux — fichiers lourds
Source: "..\src-tauri\resources\vm\*"; \
        DestDir: "{app}\vm"; \
        Flags: ignoreversion recursesubdirs createallsubdirs

; Icônes
Source: "assets\icon.ico"; \
        DestDir: "{app}"; \
        Flags: ignoreversion

; ── Icônes et raccourcis ──────────────────────────────────
[Icons]
; Bureau
Name: "{autodesktop}\{#AppName}"; \
      Filename: "{app}\{#AppExeName}"; \
      IconFilename: "{app}\icon.ico"; \
      Comment: "{#AppDescription}"

; Menu Démarrer
Name: "{group}\{#AppName}"; \
      Filename: "{app}\{#AppExeName}"; \
      IconFilename: "{app}\icon.ico"

Name: "{group}\Désinstaller {#AppName}"; \
      Filename: "{uninstallexe}"; \
      IconFilename: "{app}\icon.ico"

; ── Registre Windows ──────────────────────────────────────
[Registry]
; Ajout au PATH utilisateur (optionnel — pour CLI future)
Root: HKCU; Subkey: "Environment"; \
      ValueType: expandsz; ValueName: "Path"; \
      ValueData: "{olddata};{app}"; \
      Check: NeedsAddPath('{app}')

; Infos programme dans Ajout/Suppression de programmes
Root: HKLM; Subkey: "Software\Microsoft\Windows\CurrentVersion\Uninstall\{#AppName}"; \
      ValueType: string; ValueName: "DisplayName"; ValueData: "{#AppName}"
Root: HKLM; Subkey: "Software\Microsoft\Windows\CurrentVersion\Uninstall\{#AppName}"; \
      ValueType: string; ValueName: "DisplayVersion"; ValueData: "{#AppVersion}"
Root: HKLM; Subkey: "Software\Microsoft\Windows\CurrentVersion\Uninstall\{#AppName}"; \
      ValueType: string; ValueName: "Publisher"; ValueData: "{#AppPublisher}"
Root: HKLM; Subkey: "Software\Microsoft\Windows\CurrentVersion\Uninstall\{#AppName}"; \
      ValueType: string; ValueName: "URLInfoAbout"; ValueData: "{#AppURL}"

; ── Tâches (checkboxes pendant install) ──────────────────
[Tasks]
Name: "desktopicon";  \
      Description: "Créer un raccourci sur le Bureau"; \
      GroupDescription: "Raccourcis :"; \
      Flags: checked

Name: "startuprun";   \
      Description: "Lancer {#AppName} au démarrage de Windows"; \
      GroupDescription: "Options :"; \
      Flags: unchecked

; ── Exécution au démarrage (si coché) ─────────────────────
[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; \
      ValueType: string; ValueName: "{#AppName}"; \
      ValueData: """{app}\{#AppExeName}"""; \
      Tasks: startuprun; Flags: uninsdeletevalue

; ── Lancer l'app à la fin ────────────────────────────────
[Run]
Filename: "{app}\{#AppExeName}"; \
          Description: "Lancer {#AppName} maintenant"; \
          Flags: nowait postinstall skipifsilent; \
          Check: not IsTaskSelected('startuprun')

; ── Code Pascal — fonction NeedsAddPath ──────────────────
[Code]
// Vérifie si le chemin est déjà dans PATH avant de l'ajouter
function NeedsAddPath(Param: string): boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(
    HKEY_CURRENT_USER,
    'Environment',
    'Path',
    OrigPath
  ) then begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Param + ';', ';' + OrigPath + ';') = 0;
end;

// Message de bienvenue personnalisé
function InitializeSetup(): Boolean;
begin
  Result := True;
end;

// Vérification Windows 10 minimum
function InitializeWizard(): Boolean;
begin
  Result := True;
end;

// Confirmation avant désinstallation
function InitializeUninstall(): Boolean;
begin
  Result := MsgBox(
    'Voulez-vous vraiment désinstaller RoboLaunch ?' + #13#10 +
    'Vos projets et données ne seront pas supprimés.',
    mbConfirmation,
    MB_YESNO
  ) = IDYES;
end;

// Nettoyer les fichiers VM lors de la désinstallation (optionnel)
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    // Supprimer le dossier VM si vide
    RemoveDir(ExpandConstant('{app}\vm'));
    RemoveDir(ExpandConstant('{app}'));
  end;
end;
