# Moodle Video Bulk Downloader

[![Rust](https://img.shields.io/badge/Rust-2021-orange)](https://www.rust-lang.org/)
[![Licence: MIT](https://img.shields.io/badge/Licence-MIT-green.svg)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-bienvenues-brightgreen.svg)](CONTRIBUTING.md)

English version: [README.md](README.md)

## Avertissement

1. Cet outil sert uniquement à l'archivage personnel. Ne republiez pas les enregistrements de cours sans l'autorisation explicite de votre professeur.
2. Il ne fonctionnera pas forcément sur toutes les instances Moodle. N'hésitez pas à forker et ouvrir une pull request. *Développé à l'origine pour moodle.unine.ch.*

## Ce que ça fait

Donnez-lui une page de replay Moodle/UbiCast (ou une page de cours, ou un fichier texte listant plusieurs liens) : il se connecte avec vos cookies de session, trouve les flux HLS audio et vidéo, télécharge les deux en parallèle avec `ffmpeg`, puis les muxe en un seul MKV avec `mkvmerge`. Pas de Python, pas de `pip install`, pas d'extension de navigateur — un seul exécutable.

C'est une réécriture complète en Rust de ce qui était un script Python/Playwright. La bibliothèque principale, le CLI et la GUI partagent le même moteur de téléchargement (`crates/core`) ; un Chromium headless embarqué, piloté via le protocole DevTools, remplace ce que faisait Playwright avant.

## Interface graphique

![Capture d'écran de la GUI](docs/screenshot.png)

Collez vos cookies, collez vos liens (ou une liste au format `liens.txt`), choisissez un dossier de sortie, cliquez sur Démarrer. Le tableau se met à jour en direct pendant que chaque vidéo est extraite, téléchargée puis muxée ; les réglages sont mémorisés d'un lancement à l'autre (les cookies non — il faut les recoller à chaque fois).

## Fonctionnalités

- Télécharge un seul replay, ou pointe sur une page de cours pour découvrir automatiquement toutes les activités UbiCast/mediaserver qu'elle contient
- Analyse les fichiers d'entrée groupés (nom de cours sur sa propre ligne, URLs en dessous)
- Authentification par cookies uniquement (`cookies.txt`, ou collés directement dans la GUI)
- Choisit automatiquement la meilleure résolution vidéo disponible (ex. `1440p > 1080p`), en testant des résolutions voisines que la page elle-même ne mentionnait pas
- Téléchargement audio et vidéo en parallèle pour chaque vidéo
- Arrête d'attendre les URLs de flux dès que les deux sont trouvées, plutôt que d'attendre systématiquement le délai maximal
- Nouvelle tentative automatique avec repli exponentiel (3 essais, 5 s puis 10 s)
- Datation des fichiers : la date de modification du fichier est réglée sur la date de publication de la vidéo, récupérée sur la page (`--no-set-file-date` pour désactiver)
- Concurrence limitée entre vidéos (`--concurrency` / réglage GUI)
- Ctrl+C / Annuler tue immédiatement tous les process ffmpeg en cours

## Récupérer l'appli

Deux options : récupérer un dossier `MVBD-Portable` déjà construit (GUI + CLI + ffmpeg/mkvmerge/Chromium, rien d'autre à installer), ou le construire soi-même :

```powershell
cargo install tauri-cli --version "^2"
cargo build --workspace --release
```

## Authentification

Les cookies sont la seule méthode d'authentification supportée.

1. Connectez-vous à Moodle dans votre navigateur.
2. Ouvrez les outils de développement (`F12`) → Application/Stockage → Cookies, sélectionnez le domaine de votre Moodle.
3. Copiez au moins `MoodleSession` et le cookie de session SSO (ex. `_shibsession_...`).
4. Collez-les dans `cookies.txt`, ou directement dans le champ cookies de la GUI. Les deux formats fonctionnent :

Un par ligne :

```text
MoodleSession=...
_shibsession_...=...
```

Ou en une ligne, tel que copié depuis le navigateur :

```text
MoodleSession=...; _shibsession_...=...;
```

## Format du fichier d'entrée

Exemple `liens.txt` :

```text
Cours A
https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx
https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx
Cours B
https://moodle.unine.ch/course/view.php?id=yyyyyy
```

Une URL de page de cours (`/course/view.php?...`) est automatiquement développée en toutes les activités multimédia qu'elle contient.

## Utilisation en CLI

```powershell
mvbd-cli.exe --input liens.txt
mvbd-cli.exe --url "https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx"
mvbd-cli.exe --input liens.txt --concurrency 2
```

| Option | Défaut | Description |
| --- | --- | --- |
| `--cookie-file` | `cookies.txt` | Fichier de cookies (KEY=VALUE ou une par ligne) |
| `--concurrency` | `1` | Vidéos traitées en parallèle |
| `--download-threads` | `4` | Threads ffmpeg par flux |
| `--capture-wait-ms` | `8000` | Attente max pour capturer les URLs de flux (sort dès que trouvées) |
| `--ffmpeg-timeout` | `1800` | Timeout (secondes) par téléchargement audio/vidéo |
| `--output-dir` | `dl` | Répertoire de sortie |
| `--no-set-file-date` | désactivé | Désactive la datation des fichiers |
| `--show-browser` | désactivé | Affiche la fenêtre Chromium (debug) |
| `--keep-temp` | désactivé | Conserve les fichiers audio/vidéo séparés |
| `--dry-run` | désactivé | Affiche ce qui serait téléchargé sans télécharger |

## Construire le dossier portable

```powershell
scripts\build-portable.ps1
```

Cette commande construit le CLI et la GUI en mode release, récupère ffmpeg/mkvmerge/Chromium dans `tools/`, puis assemble un dossier `MVBD-Portable/` autonome (et un `.zip` correspondant) — aucun toolchain Rust n'est nécessaire pour l'exécuter, il suffit de double-cliquer sur `mvbd-gui.exe`.

## Sortie

`dl/<Nom du cours>/<Titre de l'enregistrement>.mkv`

## Contribuer

Voir [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

MIT — voir [LICENSE](LICENSE).
