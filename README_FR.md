# Moodle Video Bulk Downloader

[![Python](https://img.shields.io/badge/Python-3.11%2B-blue)](https://www.python.org/)
[![Licence: MIT](https://img.shields.io/badge/Licence-MIT-green.svg)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-bienvenues-brightgreen.svg)](CONTRIBUTING.md)

English version: [README.md](README.md)

## Avertissement

1. Cet outil est destiné uniquement à l’archivage personnel. Ne republiez pas les enregistrements sans l’accord explicite du professeur.
2. Il peut fonctionner ou non selon les instances Moodle. N’hésitez pas à forker le projet et proposer une pull request. *Créé pour moodle.unine.ch*

## Fonctionnalités

- Téléchargement des replays Moodle/UbiCast depuis une URL ou un fichier texte
- Support des fichiers d’entrée avec sections de cours
- Authentification uniquement par cookies (`cookies.txt`)
- Détection de `audio_*.m3u8` et meilleure variante vidéo disponible (ex: `1440p > 1080p`)
- Scan automatique des pages de cours : si vous passez l'URL d'un cours, l'outil découvrira et mettra en file tous les modules UbiCast détectés
- Téléchargement audio/vidéo avec barres de progression
- Mux final en MKV avec `mkvmerge` (MKVToolNix CLI)
- Traitement en parallèle

## Prérequis

- Python 3.11+

Installer les dépendances :

```powershell
pip install -r requirements.txt
python -m playwright install chromium
```

## Authentification

Les cookies sont la seule méthode d'authentification supportée.

### Comment récupérer les cookies Moodle

1. Connectez-vous à Moodle dans votre navigateur.
2. Ouvrez les DevTools (`F12`) puis Application/Storage -> Cookies.
3. Sélectionnez le domaine Moodle (par exemple `https://moodle.unine.ch`).
4. Copiez au minimum `MoodleSession` et le cookie de session SSO (par ex. `_shibsession_...`).
5. Collez-les dans `cookies.txt`. Les deux formats sont supportés :

Une par ligne :

```text
MoodleSession=...
_shibsession_...=...
```

Ou sur une seule ligne (copié depuis le navigateur) :

```text
MoodleSession=...; _shibsession_...=...;
```

Exemple :

```text
MoodleSession=...
_shibsession_...=...
```

## Format du fichier d’entrée

Exemple `test.txt` :

```text
Cours A
https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx
https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx
Cours B
https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx
```

## Utilisation

Depuis un fichier :

```powershell
python main.py --input test.txt --cookie-file cookies.txt
```

Depuis une ou plusieurs URL directes :

```powershell
python main.py --url "https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx"
```

## Options utiles

| Option | Valeur par défaut | Description |
| --- | --- | --- |
| `--concurrency` | `1` | Nombre de liens traités en parallèle |
| `--download-threads` | `1` | Threads ffmpeg par flux |
| `--capture-wait-ms` | `8000` | Attente après clic Play pour capter les flux |
| `--ffmpeg-timeout` | `1800` | Timeout (secondes) par téléchargement audio/vidéo |
| `--output-dir` | `dl` | Dossier de sortie |
| `--show-browser` | `false` | Affiche le navigateur pour debug auth |
| `--keep-temp` | `false` | Conserve les fichiers audio/vidéo séparés |
| `--dry-run` | `false` | Affiche ce qui serait téléchargé sans effectuer le téléchargement |

## Build EXE (PyInstaller)

Commande de compilation :

```powershell
compile.cmd
```

Sortie :

- `dist/moodle-video-bulk-downloader/moodle-video-bulk-downloader.exe` (avec `tools/ffmpeg.exe` et `tools/mkvmerge.exe` intégrés)

Le dossier de l'EXE contient toutes les dépendances et outils — aucune installation supplémentaire nécessaire.


## Sortie

- `dl/<Nom du cours>/<Titre du replay>.mkv`

Les binaires nécessaires sont placés directement dans `tools/` (`mkvmerge.exe`, `ffmpeg.exe`).

## Contribution

Consultez [CONTRIBUTING.md](CONTRIBUTING.md) pour les règles de contribution.

## Licence

Ce projet est sous licence MIT. Voir [LICENSE](LICENSE).
