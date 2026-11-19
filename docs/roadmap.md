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
- Noyau à la Pi : lire, écrire, modifier un fichier de son atelier, exécuter une commande PowerShell (accord de Julien sauf lecture simple).
- Outils fabriqués par Oz : script PowerShell plus test, adopté seulement si le test passe après accord.
- Modes de déplacement : libre, fixe, ancré au bord, concentration ; devant ou derrière les fenêtres.
- Apparence : couleur et texture de la coquille.
- Interrompre Oz à la voix ; garde anti-écho qui suit la latence de sortie.
- Émotions : le visage change (12 expressions) et des bulles d'émoji s'envolent ; Gemini choisit l'émotion.
- Personnalité plus vivante (humeurs, caprices, jeux où il laisse chercher), toujours dans la langue choisie.
- Mémoire traitée en fin de conversation ; un fait qui en contredit un autre le remplace.
- Garde-fou du regard : après « arrête de regarder », plus aucune capture sans un geste de Julien.
- Son des autres applis coupé pendant la conversation, remis même après un arrêt forcé d'Oz.
- Outils rangés en toolsets (à la façon d'Hermes Agent), activables dans l'onglet Outils ; noms d'outils en anglais.
- Modules installables (git ou dossier) : outils PowerShell et skills, code relu avant installation ; module d'exemple `window-sizes`.

## Ensuite

- Fusionner la branche `feat/skills-volume-fixes` dans `main`.
- Oz réagit de lui-même avec une émotion à ce qu'il voit ou entend (aujourd'hui, seulement quand il répond).
- Oz réagit de lui-même à ce qu'il voit pendant « Regarde avec moi ».
- Serveurs MCP : brancher les outils de l'écosystème MCP comme des toolsets, chaque appel passant par le broker.
- Skills au format standard (agentskills.io, compatible Hermes) : importer ceux de la communauté.
- Hooks : du code de module appelé en début et fin de conversation, avant une réponse.
- Les outils fabriqués par Oz enregistrés au format module.
- Outils pour les modèles locaux (Ollama), pour retirer le dernier routeur par mots-clés.
- Vérifier si la reprise de session Live accepte une nouvelle liste d'outils (aujourd'hui, les outils fabriqués passent par `use_tool`).

## Plus loin

- Reconnaître la voix de Julien : empreinte vocale locale (ECAPA-TDNN en ONNX), enregistrée une fois, pour écarter les autres voix avant Gemini. D'abord mesurer la vitesse sur le processeur.
- Mot d'éveil, ou appuyer pour parler.
- Croissance de l'œuf (éclosion, stades) : en attente, pas avant que Julien le redemande.
