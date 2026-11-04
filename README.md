# Wii Party Launcher

Launcher de bureau (Linux, Windows, macOS) : **Tauri 2** + **Svelte 5** + Vite.

| | Linux | Windows | macOS |
| --- | --- | --- | --- |
| Compile | oui | vérifié par `cargo check` | vérifié par `cargo check` (arm64) |
| Lancé pour de vrai | oui | **jamais** | **jamais** |
| Jeu, versions `.wpver`, mods `.wpmod` | oui | code commun, non testé | code commun, non testé |
| Wiimotes connectées par le launcher, main, croix, passage au jeu sans déconnexion | oui | **non** | **non** |
| Wiimotes dans le jeu | par le launcher | par Dolphin (à appairer au système), non testé | par Dolphin, non testé |

Il n'existe de transport Bluetooth que pour Linux (`src-tauri/src/wiimote/linux.rs`) et le pont vers Dolphin
passe par une socket Unix : ailleurs, `unsupported.rs` prend le relais et le jeu démarre sans passer par le launcher
pour les manettes. Dolphin n'existe qu'en arm64 sur macOS : un Mac Intel n'a pas de quoi lancer le jeu.

## Lancer

```sh
npm install
npm run tauri dev      # appli de bureau
npm run dev            # juste l'UI dans le navigateur : http://localhost:1420
npm run tauri build    # paquets pour l'OS courant (src-tauri/target/release/bundle/)
```

### Linux (Arch)

```sh
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg
```

`tauri build` produit un `.deb`, un `.rpm` et un `.AppImage`.

### Windows / macOS

Windows : WebView2 (déjà présent sur Windows 10/11) et les outils de build C++ de Visual Studio.
macOS : les outils Xcode en ligne de commande (`xcode-select --install`).
Chaque OS se compile sur lui-même (ou via la CI) : pas de compilation croisée.
`.github/workflows/launcher.yml` lance les tests et fabrique les paquets sur les 3 OS, sans Dolphin ni version
tant que le launcher n'a pas de dépôt ni la release du fork.

## Programmes intégrés

Dolphin et wit sont livrés **dans** le launcher : rien à installer au premier lancement, ça marche
hors ligne. La liste vient de `src-tauri/tools.json` (un bloc par OS) :

| Programme | Rôle | Linux | Windows | macOS |
| --- | --- | --- | --- | --- |
| Dolphin | lance le jeu, Wiimotes | `.tar.gz` portable (x86_64) | `.zip` (x64) | `.tar.gz` de `Dolphin.app` (arm64) |
| wit 3.05a | reconstruire l'ISO | `.tar.gz` (x86_64) | `.zip` Cygwin | `.tar.gz` |

Dolphin vient de notre fork [prstance/wp-dolphin](https://github.com/prstance/wp-dolphin)
(branche `wp/main`, voir `Tools/wp/README.md` là-bas) : même version sur les 3 OS, compilée par
GitHub Actions et publiée en release avec ses SHA-256.

- `scripts/fetch-tools.mjs` (`npm run tools`, lancé tout seul avant `tauri dev` et `tauri build`)
  télécharge l'archive de l'OS courant, vérifie son SHA-256, la décompresse dans
  `src-tauri/tools/<id>/` avec un `installed.json` (version, exécutable). Tauri embarque ce dossier
  comme ressource. Rien n'est refait si la bonne version est déjà là.
- `tauri build` passe `--strict` : chaque archive doit avoir son `sha256`. En dev, un `sha256` à
  `null` est accepté (le script affiche la somme à reporter), et si le téléchargement échoue on garde
  la version déjà présente.
- `WP_TOOL_DOLPHIN=chemin/vers/archive npm run tools` utilise une archive locale (artefact de CI…).
- Code : `src-tauri/src/tools.rs` (commande `tools_status`, `tools::find` pour lancer un programme) ;
  en dev les programmes tournent depuis `src-tauri/tools/`, une fois installé depuis les ressources.
- Test réel (lit BlueZ) : `cd src-tauri && cargo test -- --ignored`.

**À chaque nouvelle release du fork** : reporter les URL et les sommes de `SHA256SUMS.txt` dans
`src-tauri/tools.json` et changer `version`.

## Wiimotes

Le launcher se connecte lui-même aux Wiimotes (écran Manettes), comme le fait Dolphin : BlueZ
sert à les découvrir, puis le launcher ouvre les deux canaux HID L2CAP (PSM 0x11 et 0x13).

- Linux uniquement pour l'instant (crate `bluer`, service `bluetoothd` requis) ; ailleurs l'écran
  affiche « Pas encore disponible ».
- 4 places J1–J4 : LED du joueur, batterie (relue toutes les minutes), extension détectée
  (Nunchuk, Classique…), test des boutons en direct, vibration pour identifier, changement de place.
- Une Wiimote déjà appairée qui se reconnecte toute seule est prise par le pilote HID du système :
  l'écran la signale et propose de la libérer (déconnexion BlueZ), puis on appuie sur 1 et 2.
- La Wiimote J1 pilote le launcher : caméra IR activée (mode étendu, rapport 0x33), la visée
  calculée à partir des deux points de la sensor bar déplace une main ; croix = passer d'un bouton
  à l'autre, A = valider, B = retour, HOME = accueil (`src/lib/remote-nav.svelte.js`). Il faut une
  sensor bar allumée ; sa position (au-dessus / en dessous de l'écran) se règle dans Manettes, avec
  une vue de ce que voit la caméra de J1. Amplitude de visée : `SPAN_X`, `SPAN_Y` en haut du fichier.
- Sans sensor bar (aucun point IR depuis 5 s), le MotionPlus prend le relais : il est détecté
  (registre 0xA600FA) et activé à la connexion, les données arrivent dans le rapport 0x37 (IR de
  base + 6 octets d'extension) et la main suit le gyroscope comme une souris. Le zéro du gyroscope
  se recalibre dès que la Wiimote est immobile. Réglages : `GYRO_SPAN_X`, `GYRO_SPAN_Y`.
  MotionPlus actif, un Nunchuk branché dessus est vu comme « Extension » sans son type.
  Un bouton dessiné avec une pastille A, B… (span `aria-hidden` au texte « A ») répond au bouton
  correspondant de la Wiimote ; A presse d'abord ce que la main ou la croix a sélectionné.
- Pendant la partie, elles restent connectées au launcher, à leur place : voir « Lancer le jeu ».
- Code : `src-tauri/src/wiimote/` (commandes `wiimote_*`, évènement `wiimotes://state`) et
  `src/lib/wiimotes.svelte.js`.

## Lancer le jeu

Jouer → choix de l'ISO → vérification → Dolphin (`src-tauri/src/game.rs`, `src/lib/game.svelte.js`).

- L'ISO se choisit avec l'explorateur ou par glisser-déposer ; `dolphin-tool header` refuse tout ce
  qui n'est pas Wii Party (identifiant `SUP…`). Formats : iso, wbfs, rvz, gcz, wia, ciso.
- Il faut au moins une Wiimote connectée, sauf là où le launcher ne sait pas encore en connecter
  (Windows, macOS) : `Wiimotes::has_player`, côté écrans `hasPlayer()`.
- `dolphin-tool verify` contrôle l'image si le réglage est actif. Une image moddée y signale des
  anomalies : le joueur choisit alors « Lancer quand même » ou un autre ISO.
- Les Wiimotes ne se déconnectent pas : le launcher ouvre une socket locale et la passe à Dolphin
  (`WP_WIIMOTE_BRIDGE`, `IOBridge.*` dans le fork), qui reçoit les rapports de chaque place J1–J4 au
  lieu de chercher des manettes en Bluetooth. Une Wiimote qui décroche pendant la partie retrouve sa
  place avec 1 et 2.
- Pendant la partie, l'écran Manettes fonctionne aussi. Changer de place ferme la socket des deux
  joueurs concernés : Dolphin redemande chaque place, et le jeu voit les manettes se reconnecter. La
  recherche ajoute un joueur à une place libre, puis s'arrête, car elle ralentit le Bluetooth.
- Sans sensor bar, le gyroscope remplace la barre en jeu (`src-tauri/src/wiimote/bridge.rs`) : le jeu
  voit une Wiimote simple dans le mode qu'il demande, avec deux points IR calculés à partir du
  MotionPlus, qui lui reste caché. Si la barre est vue, ses vrais points passent. Un Nunchuk branché
  ou une demande d'IR complète éteignent le MotionPlus et laissent passer la Wiimote telle quelle.
- Dolphin tourne avec son propre dossier utilisateur (`<données>/dolphin`, sauvegardes comprises) et
  reçoit les réglages en options `-C` : ils ne touchent pas à sa configuration enregistrée. Sa
  sortie va dans `launcher-dolphin.log`, dans ce dossier.
- Les réglages (`src-tauri/src/settings.rs`, `src/lib/settings.svelte.js`) sont dans `settings.json`,
  dans le dossier de configuration de l'application. Le démarrage automatique vit dans l'entrée
  système du plugin autostart : `settings.json` n'en garde qu'une copie pour l'affichage.

## Mods

Le launcher n'accepte que les `.wpmod`, déposés dans la Bibliothèque ou choisis avec « Parcourir »
(`src-tauri/src/mods.rs`, `src/lib/mods.svelte.js`, `src/components/ModDrop.svelte`). Un mod est
décompressé dans `<données>/mods/<id>/` et activé. Réinstaller le même `id` remplace l'ancienne copie.

Un `.wpmod` est un zip avec un `mod.json` à la racine :

```json
{
  "format": 1,
  "id": "auteur.mon-mod",
  "name": "Mon mod",
  "version": "1.0.0",
  "authors": ["auteur"],
  "description": "…",
  "games": ["SUPP01"],
  "requires": { "auteur.base": ">=1.0" },
  "conflicts": { "autre.mod": "*" },
  "patches": [
    {
      "type": "riivolution",
      "xml": "riivolution/mon-mod.xml",
      "options": [{ "section": "Wii Party", "option": "Mon mod", "choice": 1 }]
    }
  ]
}
```

- Seuls `format`, `id`, `name`, `version`, `games` et `patches` sont obligatoires. Une plage de versions
  vaut `*` ou `>=x.y`.
- `riivolution` est le seul type de patch pris en charge pour l'instant : un mod Gecko est refusé.
- `options` dit quelles options du XML activer (`choice` compte à partir de 1). La racine Riivolution
  est celle du paquet.
- `scripts/make-wpmod.py` fabrique un `.wpmod` à partir d'un paquet Riivolution.
- `scripts/riivolution-from-disc.py` fabrique ce paquet à partir d'une copie modifiée du disque extrait.
  Un `main.dol` modifié y entre en `<file disc="main.dol">` : Dolphin le comprend, pas une vraie Wii.

Au lancement, le launcher écrit un descripteur de mod Dolphin (`mods/.launch.json`) qui pointe vers
l'ISO et les mods activés : l'ISO n'est pas modifié. Les mods s'appliquent dans l'ordre d'activation.

Cliquer sur un mod (ou « Détails » sur une version) ouvre sa fiche, `#/ModDetail/<id>`
(`src/screens/ModDetail.svelte`) : description, réglages, ce qu'il modifie, problèmes qui le concernent.

### Textes traduits et icône

`name`, `description` et les textes des réglages acceptent une chaîne ou la même par langue du launcher :
`"name": { "fr": "Tempête de neige", "en": "Snowstorm", "es": "Tormenta de nieve" }`. Une langue absente
prend l'anglais, puis le français. `"icon": "icon.png"` (SVG ou PNG du paquet, 512 Ko au plus) s'affiche
dans la Bibliothèque et sur la fiche. Pareil pour une version.

### Réglages d'un mod

Un mod (ou une version) peut proposer ses propres réglages, que le joueur change sur sa fiche. Ils sont
enregistrés dans les paramètres du launcher et s'appliquent au lancement suivant.

```json
"settings": [
  {
    "id": "tempete",
    "type": "toggle",
    "label": { "fr": "Tempête", "en": "Storm" },
    "description": { "fr": "La neige tombe pendant tout le plateau", "en": "Snow falls over the whole board" },
    "default": 1,
    "riivolution": { "xml": "riivolution/mon-mod.xml", "section": "Wii Party", "option": "Tempête" }
  },
  {
    "id": "intensite",
    "type": "choice",
    "label": "Intensité",
    "default": 1,
    "choices": [{ "value": 1, "label": "Flocons" }, { "value": 2, "label": "Blizzard" }],
    "riivolution": { "xml": "riivolution/mon-mod.xml", "section": "Wii Party", "option": "Intensité" }
  },
  {
    "id": "de-gele",
    "type": "number",
    "label": { "fr": "Dé des joueurs gelés", "en": "Frozen players' die" },
    "min": 1, "max": 6, "step": 1, "default": 3,
    "memory": { "offset": "0x80001800", "size": 1 }
  }
]
```

- `type` : `toggle` (valeur 0 ou 1), `choice` (une des `choices`) ou `number` (de `min` à `max`, par `step`).
- `riivolution` : le réglage choisit une option d'un des XML du paquet, sans code dans le mod. Pour un
  `choice`, la valeur est le numéro du choix Riivolution (0 désactive l'option) ; un `toggle` active le
  choix `on` (1 par défaut). L'option ne doit pas être aussi dans les `options` fixes du patch.
- `memory` : la valeur est écrite en big-endian sur `size` octets (1, 2 ou 4) à `offset`, avant le
  démarrage du jeu. C'est au code du mod (dans son `main.dol`) de la lire. Le launcher l'écrit par un XML
  Riivolution qu'il génère (`mods/.settings-<id>.xml`).
- Une valeur enregistrée que le paquet ne permet plus (après une mise à jour) revient à `default`.
- La vérification de compatibilité tient compte des valeurs choisies : un réglage désactivé ne compte pas.

### Vérification de compatibilité

Elle s'appuie sur le manifeste et sur les `<file>`, `<folder>` et `<memory>` des options activées :

| Cas | Résultat |
|---|---|
| Mod fait pour une autre version du jeu | le jeu ne se lance pas |
| Dépendance absente ou trop ancienne | le jeu ne se lance pas |
| Conflit déclaré dans `conflicts` | le jeu ne se lance pas |
| Valeurs différentes à la même adresse mémoire | le jeu ne se lance pas |
| Même fichier remplacé par deux mods | avertissement, le dernier activé passe en priorité |
| Patch impossible à analyser (`search`, paramètres…) | avertissement « compatibilité inconnue » |

« Aucun conflit détecté » ne prouve pas que deux mods fonctionnent ensemble : seul un essai en jeu le peut.

## Versions

Une version (`.wpver`) est un mod assez gros pour être le jeu de départ : une seule à la fois, chargée
avant tous les mods, et elle apporte le thème du launcher (`src-tauri/src/versions.rs`,
`src/lib/versions.svelte.js`, `src/screens/Versions.svelte`). Sans version, c'est « Wii Party d'origine » :
ton ISO tel quel et les couleurs de `src/app.css`.

Le paquet est celui d'un mod, avec un `version.json` au lieu du `mod.json` et un champ `theme` obligatoire.
Les conflits et les prérequis des mods s'appliquent aussi à la version active.

```json
"theme": {
  "colors": { "accent": "#1f78c4", "shadow": "29, 92, 150" },
  "tagline": "La phrase sous le titre de l'accueil",
  "hero": [{ "image": "theme/board.svg", "x": 0, "y": 0, "size": 550, "motion": "float" }],
  "home": [{ "image": "theme/flake.svg", "x": 600, "y": 70, "size": 34, "opacity": 1, "motion": "fall" }],
  "library": []
}
```

- `colors` : les variables CSS du launcher, `#rrggbb` (`shadow` et `scrim` : trois nombres). Une couleur
  absente garde sa valeur de `src/app.css`.
- `hero` : l'illustration de l'accueil, une image par partie qui bouge, dans un cadre de 550×460. `home` et
  `library` : des images posées derrière ces écrans, en coordonnées d'un écran 1280×720. Images SVG ou PNG de
  2 Mo au plus.
- `motion` (facultatif) : `fall`, `float`, `wobble` ou `bob`, les mouvements de `src/app.css`.
- `tagline` (facultatif) : une phrase, ou la même par langue du launcher, `{ "fr": "…", "en": "…", "es": "…" }`.
  Une langue absente prend l'anglais, puis le français.
- Un thème n'est que de la donnée : pas de code ni de HTML, car la page du launcher a accès à ses commandes.
  Les images passent par `<img>`, où un SVG ne peut ni exécuter de script ni charger quoi que ce soit.

Les `.wpver` de `src-tauri/versions/` sont fournis avec le launcher : installés au premier lancement, puis
remplacés seulement si le launcher en apporte une version plus récente. Ils ne peuvent pas être retirés.
Une nouvelle installation démarre sur `prstance.mode-hiver`.

`src-tauri/versions/*.wpver` n'est pas versionné : le paquet contient des fichiers du jeu. Pour le refaire :

```sh
python3 scripts/make-wpmod.py paquet-riivolution.zip --id prstance.mode-hiver --name "Mode Hiver" \
    --version 2026.11.6 --game SUPP01 --author prstance --conflicts prstance.mode-duo='*' \
    --conflicts prstance.mode-lave='*' --theme themes/mode-hiver \
    --out src-tauri/versions/Mode-Hiver-2026.11.6.wpver
```

## Langues

Le launcher parle français, anglais et espagnol (Paramètres › Général › Langue du launcher ; « Automatique »
suit la langue du système, l'anglais si ce n'en est aucune des trois). Les textes s'écrivent en français
dans le code et passent par `t()` / `plural()` (`src/lib/i18n.svelte.js`) ; leurs traductions sont dans
`src/locales/en.js` et `src/locales/es.js`. Côté Rust, les messages passent par `tr!` et leurs traductions
sont dans `src-tauri/src/i18n.rs`. Un texte sans traduction reste en français :

```sh
node scripts/check-i18n.mjs   # liste les traductions manquantes ou inutilisées
```

## Organisation

- `src/screens/` : un écran par fichier ; l'URL `#/Library` affiche `Library.svelte`.
- `src/components/Sidebar.svelte` : la barre latérale commune (`active`, `winter`).
- `src/components/WindowControls.svelte` : boutons réduire / agrandir / fermer, ajoutés par
  `App.svelte` sur les écrans qui ne les dessinent pas eux-mêmes.
- `src/lib/window.js` : appels à la fenêtre Tauri (sans effet hors Tauri).
- `src/lib/theme.svelte.js` : thème (classique / Mode Hiver), mémorisé. Les couleurs sont des variables
  CSS dans `src/app.css`, à utiliser dans les écrans (`var(--accent)`…). Les liens disent toujours
  `#/Main` et `#/Library` ; le routeur affiche `WinterHome` et `WinterLibrary` en Mode Hiver.

La fenêtre est sans bordure système (`decorations: false`) : l'en-tête de chaque écran et une fine
bande en haut servent à la déplacer. Les écrans sont dessinés en 1280×720 et mis à l'échelle
pour remplir la fenêtre.
