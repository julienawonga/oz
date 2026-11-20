# Roadmap

État au 19 novembre 2026. Les anciennes notes de conception (avatar 3D, état de l'art de 2025) sont dans [`archive/`](archive/).

## Fait

- Voix d'abord : une seule session Gemini Live, outils compris, premier son en moins d'une seconde.
- Outils Gemini au lieu de mots-clés.
- Oz engage la conversation (signaux, jugement par Gemini, appel ou main levée).
- Raccourci global du micro, langue, figures, lieux nommés.
- « Regarde avec moi » : vue en direct d'une fenêtre autorisée.
- Permissions par fenêtre, sanctuaires, journal.
- Panneau refait (barre latérale, thème clair et sombre).
- Volume du système à la voix, et baisse automatique des autres applis pendant la conversation.
- Skills : savoir-faire écrits, ajoutés depuis le panneau ; Oz en propose à partir de ses ratés répétés.
- Noyau à la Pi : lire, écrire, modifier un fichier de son atelier, exécuter une commande PowerShell (accord de l'utilisateur sauf lecture simple).
- Outils fabriqués par Oz : script PowerShell plus test, adopté seulement si le test passe après accord.
- Modes de déplacement : libre, fixe, ancré au bord, concentration ; devant ou derrière les fenêtres.
- Apparence : couleur et texture de la coquille.
- Interrompre Oz à la voix ; garde anti-écho qui suit la latence de sortie.
- Émotions : le visage change (12 expressions) et des bulles d'émoji s'envolent ; Gemini choisit l'émotion.
- Personnalité plus vivante (humeurs, caprices, jeux où il laisse chercher), toujours dans la langue choisie.
- Mémoire traitée en fin de conversation ; un fait qui en contredit un autre le remplace.
- Garde-fou du regard : après « arrête de regarder », plus aucune capture sans un geste de l'utilisateur.
- Son des autres applis coupé pendant la conversation, remis même après un arrêt forcé d'Oz.
- Outils rangés en toolsets (à la façon d'Hermes Agent), activables dans l'onglet Outils ; noms d'outils en anglais.
- Modules installables (git ou dossier) : outils PowerShell et skills, code relu avant installation ; module d'exemple `window-sizes`.
- Prénom de l'utilisateur en réglage, injecté dans tous les textes envoyés au modèle (comme la date et l'heure) ; demandé par Oz s'il manque.
- Skills (procédures écrites) et outils (actions) séparés dans le panneau et dans les consignes.
- Session vocale qui repart d'elle-même quand la reprise a expiré ; outil des émotions non bloquant (plus de phrase répétée).
- Dépôt public, historique nettoyé.

## Ensuite

Dans l'ordre :

1. **Serveurs MCP** : brancher les outils de l'écosystème MCP comme des toolsets, chaque appel passant par le broker.
2. **Skills au format standard** (agentskills.io, compatible Hermes) : importer et partager des skills.
3. **Hooks de modules** : du code appelé en début et fin de conversation, avant une réponse.
4. **Réactions spontanées** : une émotion ou une remarque d'Oz sur ce qu'il voit (« Regarde avec moi ») ou entend, sans attendre une question.

À ranger en chemin :

- Enregistrer les outils fabriqués par Oz au format module.
- Vérifier si la reprise de session Live accepte une nouvelle liste d'outils (aujourd'hui, les outils fabriqués passent par `use_tool`).
- Moins de mises en sourdine par erreur (« je devrais dormir » compris comme « laisse-moi tranquille »).

## Plus loin

- Reconnaître la voix de l'utilisateur : empreinte vocale locale (ECAPA-TDNN en ONNX), enregistrée une fois, pour écarter les autres voix avant Gemini. D'abord mesurer la vitesse sur le processeur.
- Mot d'éveil, ou appuyer pour parler.
- macOS et Linux.
- Croissance de l'œuf (éclosion, stades) : en attente.
