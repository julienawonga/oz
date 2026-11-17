# Permissions

Les permissions sont appliquées dans le code Rust, module `broker.rs`. Aucun prompt ne peut les modifier.

## Niveaux

| Niveau | Effet |
|---|---|
| `sanctuary` | La fenêtre est invisible pour Oz : aucune lecture, aucune action. La réponse est « introuvable », identique à une fenêtre qui n'existe pas. |
| `none` | Refus. Oz ne regarde pas et n'agit pas. |
| `look` | Oz peut décrire la fenêtre (capture). |
| `act` | Oz peut aussi agir : fermer, mettre au premier plan. |
| `trust` | Autorisation durable, sans nouvelle demande. |

## Clés et règles

Une règle est identifiée par `processus::titre`, par exemple `notepad.exe::Sans titre`. La clé `processus::*` est une règle pour toute l'application.

`level_for` choisit le niveau applicable avec cette priorité :

1. Si la fenêtre ou l'application est en `sanctuary`, c'est le niveau retenu, quoi qu'il y ait ailleurs.
2. Sinon, la règle de la fenêtre précise.
3. Sinon, la règle de toute l'application.
4. Sinon, aucune règle : Oz doit demander.

## Demandes d'accès

Quand une action vise une fenêtre sans règle suffisante, le broker émet `oz://access-request`. L'œuf affiche un badge sur sa coque ; un clic ouvre la carte avec les choix. La demande peut être rejouée si elle a été manquée. Un choix peut s'appliquer à la fenêtre ou à toute l'application.

## Intentions

Le routeur `detect_action` reconnaît trois verbes : *lister*, *fermer*, *focus*. La cible est extraite de la phrase en ignorant les mots génériques (« fenêtre », « programme », « application », « app ») : « ferme la fenêtre notepad » vise `notepad`.

La parole passe par le même routeur que le texte : on ne contourne donc pas le broker en parlant.

## Journal

Chaque demande, décision et action est écrite dans le journal (onglet *Activité*). Le panneau permet de modifier ou retirer une règle à tout moment (onglet *Permissions*).
