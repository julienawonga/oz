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

## Conventions

- **Sécurité dans le Rust.** Une règle de permission ne s'écrit jamais dans un prompt ni côté interface.
- **Un module, une responsabilité.** Côté Rust comme côté TypeScript, ajouter un fichier plutôt que gonfler `lib.rs` ou `main.ts`.
- **Interface sobre.** Pas d'emoji dans le panneau d'administration. Les textes parlés ou affichés par Oz sont courts (bulle de 1 à 2 phrases).
- **Événements préfixés `oz://`.** Voir [architecture](architecture.md).
- **Commits thématiques**, sur une branche, fusionnés avec `--no-ff`.

## Ajouter une action

1. Ajouter le verbe dans `detect_action` (`broker.rs`) et un test.
2. Gérer l'action dans `execute_action`, derrière le contrôle de niveau.
3. Aucun changement de prompt : le modèle ne décide pas des actions.

## Ajouter un onglet au panneau

Créer `app/src/panel/<onglet>.ts` avec une fonction `render…`, ajouter le bouton dans `panel.html` et la ligne de routage dans `panel.ts`.

## Données locales et réinitialisation

Dossier : `%APPDATA%\com.julienawonga.oz`. Pour repartir de zéro, fermer Oz et supprimer ce dossier. Pour retirer la clé API : Gestionnaire d'identifiants Windows, entrée `oz`, ou vider le champ dans l'onglet *Modèles*.

## Dépannage

| Symptôme | Piste |
|---|---|
| `ENOENT package.json` | Tu n'es pas dans `app/`. |
| Pas de réponse de l'IA | Onglet *Modèles* : fournisseur choisi, clé saisie, Ollama lancé si local. Les erreurs HTTP s'affichent avec le code et la cause. |
| Oz ne retrouve pas ses données | Vérifier que `com.julienawonga.nid` existait et que `com.julienawonga.oz` n'existait pas avant le premier lancement : la migration ne copie que vers un dossier absent. |
| Le micro coupe trop vite | Voir [voix](voix.md), section Réglages. |
