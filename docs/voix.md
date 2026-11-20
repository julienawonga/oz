# Voix

## Principe

Oz parle en direct avec **une seule session Gemini Live** (`gemini-3.8-live`, session « conversation »). Le micro y est streamé ; le modèle comprend, appelle les outils et répond directement en audio. Il n'y a pas d'étape texte entre ta phrase et sa voix : le premier son arrive en moins d'une seconde.

Le broker reste souverain : les outils appelés par Live passent par `tools::run`, exactement comme ceux du chat écrit. Le modèle propose, le code décide.

Une seconde session, « parole », sert seulement à lire à voix haute les réponses du cerveau texte (messages du panneau, reprise après une permission accordée).

## Chaîne complète

1. Le micro envoie l'audio en PCM 16 kHz (`mic_chunk`). Si la session est fermée, les morceaux sont mis en attente le temps de la rouvrir : aucun mot perdu.
2. La session est ouverte avec le prompt d'Oz (`brain::live_system_prompt` : personnalité, langue, heure, mémoire), les outils (`tools::declarations`), les transcriptions entrée et sortie, la compression du contexte et la reprise de session.
3. Les appels d'outils (`toolCall`) sont exécutés à part, puis renvoyés (`toolResponse`).
4. L'audio d'Oz arrive en morceaux (`oz://voice-talk-audio`) et est joué au fil de l'eau ; sa transcription (`oz://voice-talk-text`) s'affiche dans la bulle si le réglage le permet.
5. En fin d'échange (`turnComplete`), l'échange est rangé dans l'historique, écrit dans le journal (`conversation : <prénom> : « … » — Oz : « … »`) et compte pour la consolidation de la mémoire (tous les 6 échanges).

Le serveur ferme la session après environ 2 minutes sans parole. Elle se rouvre au prochain morceau de micro, avec la poignée de reprise : Oz garde le fil.

## Ouvrir et fermer le micro

| Ouvrir | Fermer |
|---|---|
| Clic sur l'œuf | Clic sur l'œuf |
| Raccourci global (**Ctrl+Shift+Espace** par défaut, réglable) | Le raccourci |
| Oz t'appelle de lui-même (voir *Initiative*) | « Tais-toi », « c'est fini », « pas maintenant » : outil `end_conversation` ou `not_now`, le micro se coupe après sa réponse |
| | 8 s de silence après une phrase, 15 s si rien n'a été dit |

Si Oz parle, un clic ou le raccourci lui coupe la parole. Échap coupe sa voix et annule une prise de parole en cours, sans fermer le micro.

Le silence est détecté localement au volume (`VOICE_RMS`, 0,015, dans `mic.ts`) : un bruit de fond fort peut garder le micro ouvert.

Pendant qu'Oz parle, son écho n'est pas envoyé. La garde dure jusqu'à la fin réelle de sa voix : l'audio planifié, plus la latence de sortie de la carte son (jusqu'à environ 300 ms en Bluetooth), plus 300 ms pour l'écho de la pièce.

**Lui couper la parole.** Un analyseur mesure ce qu'Oz joue. Le micro apprend en continu le rapport entre son écho et sa voix. Quand le micro dépasse nettement l'écho attendu (trois fois) pendant environ 250 ms, c'est l'utilisateur qui parle : Oz se tait, et les derniers morceaux de micro gardés en réserve partent, pour que le début de la phrase ne soit pas perdu. Réglage *Lui couper la parole* pour le désactiver si des enceintes très fortes le font s'interrompre seul.

**Baisse automatique.** Dès que le micro s'ouvre, les autres applis qui jouent du son (une vidéo, de la musique) sont coupées, chacune à part ; la voix d'Oz n'est pas touchée. Elles remontent après sa dernière phrase, et à la fermeture d'Oz. Sans ça, une vidéo couvrait la voix de l'utilisateur et Gemini répondait à la vidéo.

Pendant l'écoute, l'œuf a un halo rouge et les yeux grands ouverts.

## Langue

Choisie dans l'onglet *Réglages*. Elle est imposée à la voix (`speechConfig.languageCode`), au prompt (« Réponds toujours en … »), à la vision et à l'initiative. Changer de langue ferme la conversation vocale, qui repart de zéro.

## Initiative : Oz engage la conversation

Module `initiative.rs` (backend) et `egg/initiative.ts` (mise en scène).

1. **Signaux** mesurés toutes les 15 s, sans mot-clé : premier usage du jour, retour après 15 min d'absence (clavier et souris), 50 min de travail sans échange.
2. **Garde-fous** dans le code : réglage coupé, « pas maintenant » (60 min par défaut), conversation dans les 3 dernières minutes, 12 min entre deux initiatives (+ 25 min par appel ignoré d'affilée), 3 par heure, fenêtre en plein écran. Chaque blocage est écrit dans le journal avec sa raison.
3. **Jugement** par Gemini (contexte : signal, heure, fenêtre active hors sanctuaire, mémoire, derniers échanges) : `speak_up` ou `stay_silent`.
4. **Mise en scène** :
   - *appeler* : micro ouvert, Oz appelle l'utilisateur par son prénom jusqu'à trois fois (consignes `[Note interne]` envoyées par `live_prompt`) ; dès que l'utilisateur répond, il enchaîne sur son sujet ;
   - *lever la main* : point bleu qui pulse au-dessus de l'œuf et deux notes douces ; un clic et il parle ; ignoré au bout de 90 s.
5. **Issue** rendue au backend (`initiative_result` : répondu, ignoré, occupé), qui se fait plus discret quand l'utilisateur ne répond pas.

Le bouton *Essayer maintenant* du panneau passe les garde-fous de rythme (pas celui de la conversation en cours).

## Regarde avec moi : la vue en direct

Module `watch.rs`. L'utilisateur demande « regarde Chrome avec moi » : l'outil `watch_with_me` (action *commencer*) choisit la fenêtre (vide = celle qu'il utilise) et vérifie qu'Oz a le droit de la regarder, exactement comme la vision.

- Une image JPEG par seconde part dans la session vocale (`realtimeInput.video`), **seulement pendant une conversation** (un morceau de micro dans les 20 dernières secondes). Sans conversation, rien n'est envoyé et la session n'est pas gardée ouverte.
- Oz répond sur ce qu'il voit sans capture à la demande : premier son en moins d'une seconde, contre environ 5 s avec `look_at_window`.
- La capture tourne hors du fil asynchrone (`spawn_blocking`) : elle prend jusqu'à 1 s en mode développement.
- Le broker est consulté **à chaque image** : un accès retiré ou un sanctuaire arrête la vue sur-le-champ.
- Arrêt automatique : fenêtre fermée, 5 captures ratées d'affilée, ou 20 minutes sans conversation. Une fenêtre réduite n'est pas capturée, la vue continue.
- Témoin : un petit œil bleu qui pulse à côté de l'œuf (`oz://watch`), avec le nom de la fenêtre au survol.
- « Arrête de regarder » : action *arreter*. Une seule fenêtre à la fois.

Le prompt vocal indique la fenêtre partagée quand la session s'ouvre pendant une vue en direct.

## Réglages

| Besoin | Où |
|---|---|
| Langue, raccourci du micro, transcription dans la bulle, initiative | Onglet *Réglages* (`prefs.json`) |
| Le micro coupe trop tôt, ou ne coupe jamais | `VOICE_RMS`, `SILENCE_AFTER_SPEECH_MS`, `SILENCE_BEFORE_SPEECH_MS` dans `app/src/egg/mic.ts` |
| Seuils de l'initiative | Constantes en tête de `app/src-tauri/src/initiative.rs` |
| Voix (Kore par défaut), clé API | Onglet *Modèles* (la clé est dans le Gestionnaire d'identifiants Windows) |

## Sondes hors application

- `cargo run --example talk_probe [question]` : session Live unique avec un outil factice, chronométrée. `LANG_CODE=fr-FR` impose la langue.
- `cargo run --example watch_probe <fenêtre> [question]` : trois images d'une fenêtre dans la session Live, puis une question, chronométrée.
- `cargo run --example voice_probe [fichier.wav]` : transcription.

## Limites connues

- On ne peut pas interrompre Oz à la voix : le micro est muet pendant qu'il parle.
- L'heure et la mémoire du prompt vocal sont figées à l'ouverture de la session.
- `look_at_window` (capture ponctuelle) passe encore par le modèle texte, environ 5 s ; la vue en direct évite ce délai.
- La vue en direct ne réagit que quand l'utilisateur parle : Oz ne signale pas de lui-même ce qu'il voit.
