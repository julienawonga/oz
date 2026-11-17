# Voix

## Principe

Gemini Live ne sert qu'à **transcrire** ta voix. Il ne décide pas de ce qu'Oz répond : la transcription est traitée comme du texte tapé (routeur d'intentions, broker de permissions, mémoire, cerveau). La réponse du cerveau est ensuite lue à voix haute par la voix de Live.

Si Live répond de lui-même, sa réponse spontanée est jetée. Cette règle est ce qui garde le broker souverain.

## Chaîne complète

1. Le micro envoie l'audio à la session Live (WebSocket, `inputAudioTranscription`).
2. Les fragments transcrits arrivent via `oz://user-text`.
3. Le premier son de Live, ou la fin du tour, signale que tu as fini : la phrase est routée **une seule fois** vers `converse()`.
4. `converse()` obtient la réponse du cerveau. Pour la lire, Oz demande à Live de la dire mot pour mot, par `voice_feed`, puis termine avec `voice_end`.
5. L'audio de Live est joué seulement pendant que le cerveau « possède » la voix (`voiceOwned`).

## Propriété de la voix

L'état vit dans `state.ts` et `audio.ts` :

- `voiceOwned` : le cerveau parle, l'audio de Live est joué.
- `voiceClosed` : `voice_end` est envoyé, le prochain `turn-complete` clôt notre réponse.
- `routedThisTurn` : ta phrase a déjà été envoyée au cerveau pour ce tour.
- Un minuteur de sécurité de 60 s libère la voix si le serveur se tait.
- L'événement `interrupted` vide seulement la file audio, sans retirer la propriété.

## Micro et arrêt automatique

- L'écoute s'arrête après **8 s de silence** suivant une phrase, ou **15 s** si rien n'a été dit depuis l'ouverture. Oz annonce : « Je n'entends plus rien, j'arrête d'écouter. »
- La détection est locale : seuil d'énergie `VOICE_RMS` (0,015) dans `mic.ts`. Une transcription reçue compte aussi comme activité.
- Le silence n'est pas compté pendant qu'Oz parle ou réfléchit.
- Pendant l'écoute, l'œuf a un halo rouge qui pulse et les yeux grands ouverts.
- Si Oz parle, le micro est coupé pour éviter l'écho de sa propre voix. Reprendre la parole coupe la sienne.

## Réglages

| Besoin | Où |
|---|---|
| Le micro coupe trop tôt, ou ne coupe jamais | `VOICE_RMS`, `SILENCE_AFTER_SPEECH_MS`, `SILENCE_BEFORE_SPEECH_MS` dans `app/src/egg/mic.ts` |
| Modèle Live | Onglet *Modèles* |
| Clé API | Onglet *Modèles* : elle est stockée dans le Gestionnaire d'identifiants Windows |

## Limites connues

- Le comportement exact de Live (fidélité de la lecture mot pour mot, ordre des événements) n'a pas été validé sur de longues sessions.
- Il n'y a pas encore de mot d'éveil ni de touche pour parler : le micro s'ouvre depuis la saisie.
- La transcription locale (whisper) est compilée mais n'est pas branchée dans l'interface.
