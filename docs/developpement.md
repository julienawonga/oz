# Développement

## Lancer

```bash
cd app
npm install
npm run tauri dev
```

Les commandes npm se lancent depuis `app/`. Depuis la racine du dépôt, `package.json` est introuvable.

## Vérifier

```bash
cd app && npx tsc --noEmit      # types
cd app && npm run build         # tsc + build Vite
cd app/src-tauri && cargo test  # tests Rust
```

Les tests couvrent la logique pure du broker : niveaux, priorité du sanctuaire, détection d'intentions, extraction de cible, nettoyage des noms de fenêtres. Tout ce qui touche la sécurité doit avoir un test.

### Sondes hors application (`app/src-tauri/examples/`)

| Sonde | Vérifie |
|---|---|
| `talk_probe [question]` | Session Live unique avec un outil factice, chronométrée (`LANG_CODE=fr-FR` impose la langue). |
| `watch_probe <fenêtre> [question]` | Images d'une fenêtre dans la session Live, puis une question. |
| `capture_probe [filtre]` | Capture de chaque fenêtre (`CAPTURE_DIR` pour enregistrer les images). |
| `voice_probe [fichier.wav]` | Transcription. |

Lancement : `cargo run --example <sonde>` depuis `app/src-tauri`.

### Pendant `npm run tauri dev`

Tout fichier modifié sous `src-tauri/`, y compris un exemple, recompile et relance Oz ; un fichier enregistré à moitié écrit fait échouer la compilation. Pour un chantier Rust de plusieurs fichiers, arrêter le serveur, coder, vérifier avec `cargo check`, puis relancer. Le rechargement à chaud affiche parfois « IPC custom protocol failed » : préférer alors un redémarrage complet.

## Conventions

- **Sécurité dans le Rust.** Une règle de permission ne s'écrit jamais dans un prompt ni côté interface.
- **Un module, une responsabilité.** Côté Rust comme côté TypeScript, ajouter un fichier plutôt que gonfler `lib.rs` ou `main.ts`.
- **Interface sobre.** Pas d'emoji dans le panneau d'administration. Les textes parlés ou affichés par Oz sont courts (bulle de 1 à 2 phrases).
- **Événements préfixés `oz://`.** Voir [architecture](architecture.md).
- **Commits thématiques**, sur une branche, fusionnés avec `--no-ff`.

## Ajouter un outil

Les outils servent au chat écrit comme à la voix (Gemini comprend la demande et choisit l'outil, sans mot-clé).

1. Déclarer l'outil dans `tools::declarations` (`tools.rs`) : nom, description claire de *quand* l'utiliser, paramètres.
2. L'exécuter dans `dispatch`. Tout ce qui touche une fenêtre passe par le broker (`execute_action`, `lookable_window`, `can_look`) : la permission se vérifie dans le code, jamais dans la description.
3. Un outil qui agit sur l'œuf émet `oz://do` ; l'œuf le traite dans `egg/intents.ts`.
4. Le résultat est un texte court que le modèle reformule. Le mentionner dans `SYSTEM_PROMPT` (`brain.rs`) si Oz doit savoir qu'il en est capable.

Le routeur par mots-clés de `broker.rs` (`detect_action`) ne sert plus qu'aux modèles locaux (Ollama), qui n'ont pas encore d'outils.

## Ajouter un onglet au panneau

Créer `app/src/panel/<onglet>.ts` avec une fonction `render…`, ajouter le bouton (icône SVG et libellé) dans le bon groupe de la barre latérale de `panel.html`, et la ligne de routage dans `panel.ts`. Pour un réglage, utiliser les lignes `.setting` (libellé à gauche, commande à droite) et l'interrupteur `.switch`.

Pour vérifier le rendu sans l'application, une page temporaire peut simuler `window.__TAURI_INTERNALS__.invoke` avec des données de test, puis être capturée avec Edge en mode headless (`msedge --headless=new --screenshot`) via le serveur Vite (`http://localhost:1420`).

## Données locales et réinitialisation

Dossier : `%APPDATA%\com.julienawonga.oz`. Pour repartir de zéro, fermer Oz et supprimer ce dossier. Pour retirer la clé API : Gestionnaire d'identifiants Windows, entrée `oz`, ou vider le champ dans l'onglet *Modèles*.

## Dépannage

| Symptôme | Piste |
|---|---|
| `ENOENT package.json` | Tu n'es pas dans `app/`. |
| Pas de réponse de l'IA | Onglet *Modèles* : fournisseur choisi, clé saisie, Ollama lancé si local. Les erreurs HTTP s'affichent avec le code et la cause. |
| Oz ne retrouve pas ses données | Vérifier que `com.julienawonga.nid` existait et que `com.julienawonga.oz` n'existait pas avant le premier lancement : la migration ne copie que vers un dossier absent. |
| Le micro coupe trop vite, ou reste ouvert | Voir [voix](voix.md), section *Ouvrir et fermer le micro*. |
| Le raccourci du micro ne marche pas | Une autre application l'utilise sans doute : le changer dans *Réglages* (un message dit s'il est refusé). |
| Oz ne parle plus après Échap | Il reprend dès que Julien reparle. |
