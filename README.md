# Oz

Un petit œuf qui vit sur le bureau Windows. Il t'écoute, te répond à voix haute, et n'agit sur tes fenêtres qu'avec ta permission.

## Principe : le contrat de confiance

- **Rien sans accord.** Chaque nouvelle fenêtre déclenche une demande d'accès : *Regarder*, *Agir*, *Pas maintenant* ou *Sanctuaire*.
- **Les sanctuaires sont inviolables.** Les permissions sont appliquées dans le code Rust (le *broker*), jamais dans un prompt. Un modèle de langage ne peut donc pas les contourner. Une fenêtre en sanctuaire est traitée comme « introuvable ».
- **Quittances.** Ce qu'Oz a vu, fait et obtenu comme autorisations est consigné dans le journal d'activité.
- **Le modèle ne fait que converser.** Les actions (lister, fermer, mettre au premier plan) sont décidées par un routeur d'intentions déterministe, pas par le modèle.

## Fonctionnalités

- Œuf animé en surcouche transparente, qui se déplace sur l'écran et évite les zones interdites.
- Saisie à la demande et bulle de dialogue collée à l'œuf.
- Voix : micro avec arrêt automatique après silence, halo rouge pendant l'écoute, réponse lue à voix haute.
- Mémoire : faits durables issus des conversations et notes sur les fenêtres observées, effaçables un par un.
- Panneau d'administration : conversation, permissions, zones, modèles, mémoire, activité.
- Cerveau interchangeable : Gemini (streaming) ou Ollama en local.

## Pile technique

- **Tauri 2** : backend Rust, interface Vite + TypeScript, sans framework.
- **Cerveau** : Gemini ou Ollama, choisis dans le panneau *Modèles*.
- **Voix** : Gemini Live (transcription) + lecture de la réponse du cerveau.
- **Secret** : la clé API Gemini est dans le Gestionnaire d'identifiants Windows, jamais dans un fichier.

## Installation et lancement

Prérequis : Windows 10 ou 11, Node.js, Rust (toolchain stable) et les prérequis Tauri 2 pour Windows.

```bash
cd app
npm install
npm run tauri dev
```

Il faut se placer dans `app/` : le `package.json` n'est pas à la racine.

Au premier lancement, ouvre le panneau d'administration, onglet **Modèles**, pour choisir le cerveau et saisir la clé Gemini si besoin.

## Tests

```bash
cd app/src-tauri
cargo test    # broker, routeur d'intentions, nettoyage des noms
cd .. && npx tsc --noEmit
```

## Structure

```
app/
├── src/egg/        l'œuf : fenêtre, bulle, saisie, voix, mouvement, demandes d'accès
├── src/panel/      le panneau d'administration, un module par onglet
└── src-tauri/src/  Rust : broker, brain, voice, memory, vision, zones, journal
docs/               architecture, permissions, voix, développement
```

## Documentation

- [Architecture](docs/architecture.md) : modules, flux d'un message, événements.
- [Permissions](docs/permissions.md) : niveaux, règles, comportement du broker.
- [Voix](docs/voix.md) : pipeline micro, transcription, lecture, arrêt automatique.
- [Développement](docs/developpement.md) : conventions, tests, données locales, dépannage.
- [Concept](docs/concept.md), [feuille de route](docs/roadmap.md) et [état de l'art](docs/state-of-the-art.md) : réflexions de départ, en anglais, en partie dépassées.

## Ancien nom

Le projet s'appelait Nid. Au premier lancement, Oz reprend les données de l'ancien dossier et la clé API de l'ancienne entrée du Gestionnaire d'identifiants.
