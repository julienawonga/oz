# Architecture

Deux fenêtres Tauri partagent le même backend Rust :

- **L'œuf** (`index.html`, label `oz`) : surcouche transparente de 360 × 720, toujours au premier plan. L'œuf est au milieu ; la place au-dessus et en dessous accueille la bulle. Hors de l'œuf et de la bulle, les clics traversent la fenêtre.
- **Le panneau** (`panel.html`, label `panel`) : administration.

Le frontend n'a aucune logique de sécurité. Il appelle des commandes Rust (`invoke`) et écoute des événements.

## Backend (`app/src-tauri/src/`)

| Module | Rôle |
|---|---|
| `lib.rs` | Déclare les modules, migre les anciennes données, enregistre les commandes, le raccourci global et la boucle d'initiative. |
| `broker.rs` | Permissions, demandes d'accès, exécution des actions sur les fenêtres. Garde un routeur par mots-clés pour Ollama seulement. Contient les tests. |
| `tools.rs` | Outils déclarés à Gemini (chat et voix) et leur exécution. |
| `brain.rs` | Chat écrit (Gemini en SSE avec outils, Ollama en NDJSON), prompt d'Oz dans la langue choisie, clé API dans le coffre Windows. |
| `voice.rs` | Sessions Gemini Live (conversation et lecture), micro, consignes `live_prompt`, whisper. |
| `initiative.rs` | Oz engage la conversation : signaux, garde-fous, jugement par Gemini. |
| `prefs.rs` | Réglages : langue, raccourci du micro, transcription, initiative. |
| `places.rs` | Lieux nommés (« la maison »). |
| `memory.rs` | Faits durables, épisodes, souvenirs sur les fenêtres, consolidation (Gemini ou Ollama). |
| `vision.rs` | Capture d'une fenêtre autorisée et description ; résolution de fenêtre et contrôle du regard partagés (`lookable_window`, `can_look`). |
| `watch.rs` | « Regarde avec moi » : une image par seconde d'une fenêtre autorisée dans la session vocale. |
| `zones.rs` | Zones interdites à l'œuf. |
| `journal.rs` | Quittances : ce qu'Oz a vu, fait, demandé, et le texte des échanges vocaux. |
| `cursor.rs`, `windows_api.rs` | Curseur global et API Win32 (énumération, capture, fermeture, focus). |

Le client HTTP est partagé (`http_client()`, connexion 10 s, lecture 60 s) et les erreurs HTTP passent par `http_error()`, qui produit des messages lisibles.

## Frontend (`app/src/`)

`main.ts` est un point d'entrée mince. Tout est dans `egg/` :

| Module | Rôle |
|---|---|
| `state.ts` | État partagé : réflexion, voix, micro, réglages. |
| `window.ts` | Glisser-déposer, clic sur l'œuf, click-through, regard, clignement. |
| `mic.ts` | Ouverture du micro, détection de silence, halo d'écoute. |
| `audio.ts` | Lecture de la voix, transcriptions dans la bulle, événements `oz://voice-*`. |
| `initiative.ts` | Mise en scène des prises de parole d'Oz (appeler, lever la main). |
| `intents.ts` | Actions du corps demandées par les outils (`oz://do`) : déplacement, lieux, pose, figures, fin de conversation. |
| `motion.ts` | Poses, balade, glissements, bornes de l'écran calculées sur l'œuf, zones interdites. |
| `figures.ts` | Figures : trajets (zigzag, cercle, huit, vague, spirale) et numéros sur place (toupie, salto, demi-tour, saut, danse). |
| `converse.ts` | Chat texte en streaming lu par la voix (panneau, reprise après permission). |
| `access.ts` | Badge et carte de demande d'accès. |
| `ui.ts` | Bulle (au-dessus ou sous l'œuf selon la place). |

Le panneau (`panel.html`, `panel.ts`, `panel.css` et `panel/`) a une barre latérale groupée : *Oz* (`chat`, `settings`, `memory`), *Confiance* (`perms`, `zones`, `journal`), *Système* (`brain`). Un module par onglet. Le pied de la barre rappelle le raccourci du micro. Couleur d'accent : le bleu de l'œuf ; thème clair ou sombre selon Windows ; sous 720 px de large, la barre ne garde que les icônes.

## Parcours d'une parole

1. Clic sur l'œuf ou raccourci : le micro streame vers la session Live « conversation ».
2. Gemini Live comprend la demande et, si besoin, appelle un outil.
3. `tools::run` exécute l'outil. Tout ce qui touche une fenêtre passe par le broker, qui vérifie le niveau d'accès ou lève une demande d'accès. Les outils du corps (déplacement, figures) émettent `oz://do` vers l'œuf.
4. Oz répond en audio, joué au fil de l'eau.
5. L'échange est rangé dans l'historique et le journal ; tous les 6 échanges, la conversation est consolidée en faits durables.

Un message tapé dans le panneau suit le même principe avec le cerveau texte (`chat`) : mêmes outils, mêmes permissions.

## Événements

| Événement | Émis par | Usage |
|---|---|---|
| `oz://do` | outils | Actions du corps : `move`, `goto`, `pose`, `figure`, `end`. |
| `oz://initiative` | initiative | Oz veut parler : manière, sujet, première phrase. |
| `oz://shortcut-mic` | raccourci global | Même effet qu'un clic sur l'œuf. |
| `oz://watch` | watch | Vue en direct active ou non : témoin sur l'œuf. |
| `oz://prefs-updated` | prefs | L'œuf relit ses réglages. |
| `oz://access-request` | broker | Affiche la demande d'accès sur l'œuf. |
| `oz://chat-line` | œuf | Reflète la conversation dans le panneau. |
| `oz://brain-updated` | panneau | L'œuf relit la config du cerveau. |
| `oz://voice-talk-audio`, `-talk-text`, `-user-text`, `oz://voice-talk-turn` | voice | Conversation vocale : voix d'Oz, transcriptions, fin d'échange. |
| `oz://voice-audio`, `-interrupted`, `-error`, `-turn-complete` | voice | Session de lecture et état des sessions. |

## Données locales

Tout est dans le dossier de données de l'application (`%APPDATA%\com.julienawonga.oz`) : `brain.json` (sans clé), `prefs.json`, `places.json`, `permissions.json`, `zones.json`, `journal.jsonl`, mémoire (`memories.json` et les faits et épisodes consolidés), `initiative-day.txt`. La clé API est dans le Gestionnaire d'identifiants Windows, service `oz`, utilisateur `gemini_api_key`.
