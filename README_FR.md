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
- Authentification prioritaire par cookies (`cookies.txt`), avec fallback login
- Détection de `audio_*.m3u8` et meilleure variante vidéo disponible (ex: `1440p > 1080p`)
- Téléchargement audio/vidéo avec barres de progression
- Mux final en MKV avec `mkvmerge` (MKVToolNix CLI)
- Traitement en parallèle

## Prérequis

- Python 3.11+
- Accès internet

Installer les dépendances :

```powershell
pip install -r requirements.txt
python -m playwright install chromium
```

## Authentification

### Recommandé : cookies

Créer `cookies.txt` (un cookie par ligne) :

```text
MoodleSession=...
_shibsession_...=...
```

### Fallback : identifiant / mot de passe

Créer `login.txt` :

```text
email@unine.ch
mot_de_passe
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

## Sortie

- `dl/<Nom du cours>/<Titre du replay>.mkv`

Les binaires nécessaires sont placés directement dans `tools/` (`mkvmerge.exe`, `ffmpeg.exe`).

## Contribution

Consultez [CONTRIBUTING.md](CONTRIBUTING.md) pour les règles de contribution.

## Licence

Ce projet est sous licence MIT. Voir [LICENSE](LICENSE).
