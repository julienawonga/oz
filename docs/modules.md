# Modules

Un module ajoute des outils à Oz sans toucher à son code, à la façon des plugins d'Hermes Agent. Il s'installe depuis le panneau (onglet *Outils*), devient un toolset qu'on peut couper, et ses outils sont déclarés tels quels à Gemini.

Exemple complet dans le dépôt : [`examples/modules/window-sizes`](../examples/modules/window-sizes) (réduire, agrandir, restaurer une fenêtre, avec un skill).

## Contenu d'un module

```text
mon-module/
├── module.json            description et outils
├── outil.ps1              un script PowerShell (il peut servir à plusieurs outils)
└── skills/                facultatif
    └── mon-skill/SKILL.md
```

### `module.json`

```json
{
  "id": "window-sizes",
  "name": "Taille des fenêtres",
  "version": "1.0.0",
  "description": "Réduire, agrandir ou restaurer une fenêtre.",
  "author": "Oz",
  "tools": [
    {
      "name": "minimize_window",
      "description": "Réduit une fenêtre. La cible est un morceau du titre ou le nom de l'appli.",
      "parameters": {
        "type": "object",
        "properties": { "target": { "type": "string" } },
        "required": ["target"]
      },
      "script": "window.ps1"
    }
  ]
}
```

| Champ | Règle |
|---|---|
| `id` | minuscules, chiffres, tirets ; 40 caractères au plus. C'est le nom du dossier une fois installé. |
| `name`, `description`, `author`, `version` | affichés dans le panneau ; la description du module sert aussi au toolset. |
| `tools[].name` | minuscules, chiffres, `_` ; unique parmi **tous** les outils d'Oz (un module qui en masquerait un est refusé). |
| `tools[].description` | ce que lit Gemini pour savoir quand appeler l'outil : écris-la pour lui, dans la langue d'Oz. |
| `tools[].parameters` | facultatif ; un JSON Schema de type `object`, au format des déclarations de fonctions de Gemini. |
| `tools[].script` | un `.ps1` du dossier du module (pas de chemin absolu ni de `..`). |

### Les scripts

Oz lance le script avec PowerShell, sans profil ni fenêtre, dans le dossier du module, avec un délai de 25 s. Deux variables sont prêtes :

- `$OzArgs` : les arguments donnés par Gemini (objet issu du JSON), par exemple `$OzArgs.target` ;
- `$OzTool` : le nom de l'outil appelé, pour qu'un script serve à plusieurs outils.

Un script **écrit toujours sur la sortie ce qui s'est passé** (trouvé ou non, fait ou non, et pourquoi) : c'est tout ce qu'Oz reçoit pour répondre. La sortie est tronquée à 4 000 caractères ; le code de sortie est transmis.

### Les skills

Les dossiers de `skills/` suivent le format des skills d'Oz (`SKILL.md` avec un en-tête `name`, `description`). Ils apparaissent dans l'onglet *Skills* en lecture seule, avec le nom du module, et Oz les charge avec `read_skill` comme les autres.

## Installation et confiance

1. Dans l'onglet *Outils*, colle un lien git (`https://…`) ou le chemin d'un dossier, puis **Examiner**. Oz copie le module à part (`modules/.staging`) et vérifie le `module.json`. **Rien ne s'exécute.**
2. Le panneau montre ses outils, ses skills et **le code de chaque script**. **Installer** le met en place et approuve ce code exact ; **Annuler** efface tout.
3. Les scripts approuvés tournent ensuite sans redemander. Si un script change après l'installation, il ne tourne plus sans un nouvel accord sur l'œuf, code affiché. Le panneau signale le module (« script modifié »).

Réinstaller un module (même `id`) remplace l'ancienne version. Installer, désinstaller ou couper un module relance la conversation vocale, car Gemini Live fixe ses outils à l'ouverture de la session.

Les modules sont dans `%APPDATA%\com.julienawonga.oz\modules\<id>\`.

## Limites actuelles

- Scripts PowerShell seulement (pas encore de JavaScript ni de serveur MCP).
- Un script tourne avec les droits de l'utilisateur : la relecture du code à l'installation est le vrai contrôle. Les permissions par fenêtre du broker ne s'appliquent pas à ce que fait un script.
