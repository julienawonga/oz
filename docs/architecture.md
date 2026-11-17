# Architecture

Deux fenêtres Tauri partagent le même backend Rust :

- **L'œuf** (`index.html`, label `oz`) : surcouche transparente, toujours au premier plan.
- **Le panneau** (`panel.html`, label `panel`) : administration.

Le frontend n'a aucune logique de sécurité. Il appelle des commandes Rust (`invoke`) et écoute des événements.

## Backend (`app/src-tauri/src/`)

| Module | Rôle |
|---|---|
| `lib.rs` | Déclare les modules, migre les anciennes données, enregistre les commandes. |
| `broker.rs` | Permissions, demandes d'accès, routeur d'intentions, exécution des actions. Contient les tests. |
| `brain.rs` | Chat (Gemini en SSE, Ollama en NDJSON), prompt système, clé API dans le coffre Windows. |
| `voice.rs` | Session Gemini Live (WebSocket), micro, lecture de la réponse, whisper. |
| `memory.rs` | Faits durables, souvenirs sur les fenêtres, consolidation. |
| `vision.rs` | Capture d'une fenêtre autorisée et description. |
| `zones.rs` | Zones interdites à l'œuf. |
| `journal.rs` | Quittances : ce qu'Oz a vu, fait, demandé. |
| `cursor.rs`, `windows_api.rs` | Curseur global et API Win32 (énumération, capture, fermeture, focus). |

Le client HTTP est partagé (`http_client()`, connexion 10 s, lecture 60 s) et les erreurs HTTP passent par `http_error()`, qui produit des messages lisibles.

## Frontend (`app/src/`)

`main.ts` est un point d'entrée mince. Tout est dans `egg/` :

| Module | Rôle |
|---|---|
| `state.ts` | État partagé : réflexion, voix, propriété de la voix, config du cerveau. |
| `converse.ts` | Envoie un message, diffuse la réponse, la lit à voix haute. |
| `intents.ts` | Intentions locales côté interface (zones, déplacements). |
| `access.ts` | Badge et carte de demande d'accès. |
| `audio.ts` | File audio, propriété de la voix, événements `oz://voice-*`. |
| `mic.ts` | Ouverture du micro, détection de silence, halo d'écoute. |
| `ui.ts` | Gestion de la bulle. |
| `chatbar.ts`, `window.ts`, `motion.ts` | Saisie, fenêtre cliquable, déplacement de l'œuf. |

Le panneau (`panel.ts` et `panel/`) a un module par onglet : `chat`, `perms`, `zones`, `brain`, `memory`, `journal`.

## Parcours d'un message

Texte tapé ou parole transcrite suivent le même chemin :

1. `converse()` reçoit le texte.
2. Le routeur d'intentions (`detect_action`) cherche *lister*, *fermer* ou *mettre au premier plan*. Si une action est reconnue, le broker vérifie le niveau d'accès et l'exécute, ou lève une demande d'accès.
3. Sinon, la mémoire est injectée dans le contexte et le cerveau répond en streaming.
4. En mode voix, la réponse est lue à voix haute pendant qu'elle arrive.
5. Après quelques échanges, la conversation est consolidée en faits durables.

## Événements

| Événement | Émis par | Usage |
|---|---|---|
| `oz://access-request` | broker | Affiche la demande d'accès sur l'œuf. |
| `oz://chat-line` | œuf | Reflète la conversation dans le panneau. |
| `oz://brain-updated` | panneau | L'œuf relit la config du cerveau. |
| `oz://voice-audio`, `-interrupted`, `-error`, `-disconnected`, `-reset` | voice | Flux audio et état de la session Live. |
| `oz://turn-complete` | voice | Fin d'un tour Live. |
| `oz://user-text` | voice | Transcription de ta voix. |
| `oz://oz-text` | voice | Texte que Live prononce. |

## Données locales

Tout est dans le dossier de données de l'application (`%APPDATA%\com.julienawonga.oz`) : `brain.json` (sans clé), permissions, zones, journal, mémoire (`semantic.json`, `episodic.json`). La clé API est dans le Gestionnaire d'identifiants Windows, service `oz`, utilisateur `gemini_api_key`.
