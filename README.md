# Wii Party Launcher

Launcher de bureau (Linux, Windows, macOS) : **Tauri 2** + **Svelte 5** + Vite.

| | Linux | Windows | macOS |
| --- | --- | --- | --- |
| Compile | oui | vérifié par `cargo check` | vérifié par `cargo check` (arm64) |
| Lancé pour de vrai | oui | **jamais** | **jamais** |
| Jeu, versions `.wpver`, mods `.wpmod` | oui | code commun, non testé | code commun, non testé |
| Wiimotes connectées par le launcher, main, croix, passage au jeu sans déconnexion | oui | code écrit, non testé | **non** |
| Wiimotes dans le jeu | par le launcher (socket Unix) | par le launcher (pipe nommé), non testé | par Dolphin, non testé |

Le launcher a un transport Bluetooth pour Linux (`src-tauri/src/wiimote/linux.rs`, BlueZ et L2CAP) et
pour Windows (`windows.rs`, API Bluetooth de Windows et périphérique HID, jamais essayé sur une vraie
machine). Le pont vers Dolphin passe par une socket Unix sous Linux et par un pipe nommé sous Windows,
avec le même protocole. Sur macOS, `unsupported.rs` prend le relais et le jeu démarre sans passer par
le launcher pour les manettes. Dolphin n'existe qu'en arm64 sur macOS : un Mac Intel n'a pas de quoi
lancer le jeu.

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
`.github/workflows/launcher.yml` (à la main, onglet Actions) lance les tests et fabrique des paquets
sans Dolphin sur les 3 OS ; les vrais viennent de `release.yml` (voir « Mises à jour du launcher »).

## Programmes intégrés

Dolphin est livré **dans** le launcher : rien à installer au premier lancement, ça marche
hors ligne. La liste vient de `src-tauri/tools.json` (un bloc par OS) :

| Programme | Rôle | Linux | Windows | macOS |
| --- | --- | --- | --- | --- |
| Dolphin | lance le jeu, Wiimotes | `.tar.gz` portable (x86_64) | `.zip` (x64) | `.tar.gz` de `Dolphin.app` (arm64) |

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

- Linux (crate `bluer`, service `bluetoothd` requis) et Windows (`windows.rs`, jamais essayé sur une
  vraie machine) ; sur macOS l'écran affiche « Pas encore disponible ».
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
  (macOS) : `Wiimotes::has_player`, côté écrans `hasPlayer()`.
- `dolphin-tool verify` contrôle l'image si le réglage est actif. Une image moddée y signale des
  anomalies : le joueur choisit alors « Lancer quand même » ou un autre ISO.
- Les Wiimotes ne se déconnectent pas : le launcher ouvre une socket Unix locale (Windows : un pipe
  nommé `\\.\pipe\wp-launcher-<pid>`, réservé aux clients locaux, même protocole) et la passe à
  Dolphin (`WP_WIIMOTE_BRIDGE`, `IOBridge.*` dans le fork), qui reçoit les rapports de chaque place
  J1–J4 au lieu de chercher des manettes en Bluetooth. Une Wiimote qui décroche pendant la partie
  retrouve sa place avec 1 et 2. Sous Windows, si le pipe ne peut pas être créé, le launcher lâche ses
  manettes sans les déconnecter et Dolphin les ouvre lui-même.
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

## Compte et Mii

Au premier lancement, l'écran Bienvenue (`src/screens/Welcome.svelte`) crée le compte du joueur : un
Mii (un Mii de Wii Party tiré au sort, proposé d'office) et un pseudo. Tant que le joueur n'a rien
tapé, le pseudo suit le nom du Mii choisi (Matt, Shinnosuke…), ou celui du système sans Mii. « C'est
parti » suffit donc, mais on peut aussi importer un fichier Mii, en choisir
un autre ou le créer dans la Chaîne Mii. Le compte (`player` : pseudo et ID du Mii) est dans
`settings.json`. Sa tête et son pseudo remplacent le logo, dans la barre latérale et sur l'accueil.
« Mes Mii » (`src/screens/Miis.svelte`) gère la bibliothèque : renommer, exporter en `.mii`, supprimer,
choisir son Mii.

Tout vient de l'ISO du joueur. Le launcher ne livre rien de Nintendo (`src-tauri/src/mii/disc.rs`) :

- `RFL/Resource/RFLRes01.arc.lz` donne `RFL_Res.dat`, les pièces de visage avec lesquelles
  `mii/render.rs` dessine les têtes (PNG gardés dans `<données>/miis/heads`) ;
- `RFL/RSD/mii_rsd.bin.lz` donne les 100 Mii du jeu, et `locale/*/mess.bin` leurs noms (clés
  `store000`…, Matt, Shinnosuke…). En choisir un en fait une copie au nom du joueur,
  avec un nouvel ID ;
- la partition de mise à jour donne la Chaîne Mii (`_sys/RVL-NigaoeNR-v*.wad`), que Dolphin démarre
  directement. Un `.wbfs` n'a souvent plus cette partition.

Ces fichiers sont extraits avec `dolphin-tool`, une fois, dans `<données>/rfl`, dès qu'un ISO est choisi.

La bibliothèque (`<données>/miis/library.json`, 100 Mii au plus comme sur la Wii) est recopiée dans la
base Mii du NAND de Dolphin (`Wii/shared2/menu/FaceLib/RFL_DB.dat`, `src-tauri/src/mii/data.rs`) avant
chaque lancement du jeu ou de la Chaîne Mii, le Mii du joueur en premier. Wii Party les propose ainsi
comme sur une vraie Wii. La Chaîne Mii n'a pas de retour vers le launcher : pendant qu'elle tourne, le
launcher surveille la base et ferme Dolphin dès qu'un Mii y est enregistré (après 15 s de démarrage,
pendant lesquelles la chaîne peut réécrire sa base). Les Mii créés ou modifiés rejoignent alors la
bibliothèque. Un Mii supprimé dans la chaîne reste dans la bibliothèque : seul le launcher la vide.
Le format de la base suit le RFL décompilé (`wp/src/RVLFaceLib`), et une base vide écrite par le
launcher est identique, octet pour octet, à celle que crée la Chaîne Mii.

Fichiers importés : `.mii`, `.miigx` et `.rcd` (74 octets), `.rsd` (76, avec CRC), plusieurs `.rsd`
mis bout à bout, ou un `RFL_DB.dat` entier (tous ses Mii). Les Mii de 3DS, Wii U et Switch ne sont pas
convertis.

## Modes de jeu de l'accueil

Le cadre de l'accueil fait défiler les modes du jeu (Île aux défis, Globe-trotter, Roue de la chance…),
avec leur vignette et leur nom tels que le menu principal les montre (`src-tauri/src/modes.rs`,
`src/components/ModeShow.svelte`). Eux aussi viennent de l'ISO, dans la langue du launcher (`sp_EU`
pour l'espagnol, sinon l'anglais) : les vignettes de `locale/*/layout/inst/thumbnail.arc.lz` (TPL
160×120) et les noms de `locale/*/mess.bin` (clés `mn_gtex013`…). Ces deux fichiers sont gardés dans
`<données>/rfl/modes`. Le dé reste à côté du cadre.

Les vignettes du disque sont petites : pour 11 des 13 modes, le launcher montre à la place ses propres
captures, plus grandes (`src/assets/modes/<mode>.jpg`, 640×480). Contrairement au reste, celles-là sont
livrées avec le launcher. Trios de Mii et Tic-tac... réponse gardent la vignette de l'ISO. Sans ISO, le
cadre montre ces 11 captures avec leurs noms traduits dans `src/locales`.

La version active les change de deux façons :

- en remplaçant ces fichiers dans ses patchs Riivolution, comme dans le jeu : le launcher lit alors les
  siens, et une vignette changée passe devant sa capture ;
- avec `modes` dans son thème (voir Versions), pour des images faites pour le launcher, montrées en
  premier (ou seules, avec `modesOnly`). Mode Hiver ne montre que dix plans de ses parties
  (`themes/mode-hiver/mode-*.png`) : Île hivernale, 9 nouveaux mini-jeux, cases avalanche, grotte de glace, yéti, boule de neige géante, mur de glace, dé de
  glace, la glace craque, Sanctuaire du glacier.

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
- `"launcher": ">=0.2"` : les versions du launcher qu'il lui faut. Un launcher plus ancien le refuse en
  demandant d'être mis à jour (Paramètres › À propos), avant même de regarder son format ou ses patchs :
  c'est ce qui permet d'ajouter des types de patch sans casser les anciens launchers.
- Types de patch : `riivolution`, `delta` (ci-dessous), `files` et `code` (voir « Mods compatibles »). Un mod Gecko est refusé.
- `minigame_names` (facultatif) : le nom des mini-jeux qu'il ajoute ou remplace, par numéro, pour la présence Discord :
  `{ "444": { "fr": "Boule de neige", "en": "Snowball Toss" } }`. À ne pas confondre avec `minigames`, le registre des mini-jeux.
- `options` dit quelles options du XML activer (`choice` compte à partir de 1). La racine Riivolution
  est celle du paquet.
- `scripts/make-wpmod.py` fabrique un `.wpmod` à partir d'un paquet Riivolution.
- `scripts/riivolution-from-disc.py` fabrique ce paquet à partir d'une copie modifiée du disque extrait.
  Un `main.dol` modifié y entre en `<file disc="main.dol">` : Dolphin le comprend, pas une vraie Wii.

### Mods compatibles

Le launcher est aussi un éditeur de liens (`src-tauri/src/link/`, conception dans wp-core `docs/modding.md`) : un patch
`code` livre des objets compilés que le launcher relie au cœur, au lancement, dans `<données>/mods/.linked/`. Plusieurs
mods peuvent ainsi changer le même module. Le cœur (`src-tauri/core/`) est l'export de wp-core,
`build/SUPP01/wpcore/` après `configure.py --core` et `ninja`, sans `disc_mr001.rel` ; il est dans le dépôt, car la CI de
release l'embarque. Un paquet ne se lie qu'à un cœur qui a ses hooks : le changer demande de refaire les paquets.

### Deltas : pas de fichier du jeu dans le paquet

Un patch `delta` liste des fichiers du paquet que le launcher fabrique à partir de l'ISO du joueur, au
premier lancement (`src-tauri/src/rebuild.rs`, phase « preparing » de l'écran de lancement) :

```json
{ "type": "delta", "files": [
    { "disc": "/rel/mr001.rel.lz", "file": "WinterMode/rel/mr001.rel.lz", "delta": "delta/WinterMode/rel/mr001.rel.lz.wpd" },
    { "disc": "main.dol", "file": "WinterMode/sys/main.dol", "delta": "delta/WinterMode/sys/main.dol.wpd" } ] }
```

- `disc` : le fichier d'origine (`main.dol` : l'exécutable, que `dolphin-tool extract -s sys/main.dol` sait
  sortir dans notre fork). `file` : où le résultat va dans le paquet, là où son XML Riivolution l'attend.
- Un `.wpd` (`src-tauri/src/delta.rs`) : `WPD1`, un octet d'options (1 : `.lz`), le SHA-256 du fichier
  d'origine et celui du résultat, puis un patch bsdiff 4.x. Pour un `.lz`, le delta est fait entre les
  contenus décompressés, et le launcher recompresse (`src-tauri/src/lz.rs`, LZ77 type 0x11) : un delta
  entre deux fichiers compressés serait aussi gros que le fichier.
- Un ISO dont le fichier n'est pas celui d'origine (autre région, disque moddé) est refusé avec un message
  clair. Les fichiers faits restent dans le dossier du paquet ; une mise à jour du paquet les refait.
- `scripts/make-wpmod.py … --disc <image du disque d'origine>` fait les deltas tout seul : chaque fichier
  du paquet qui remplace un fichier du disque devient un delta (`src-tauri/examples/wpdelta.rs`, vérifié
  en le réappliquant), et le paquet demande `"launcher": ">=0.2"`. Mode Hiver : 135 Mo → 38 Mo, sans plus
  aucun fichier du jeu ; 183 fichiers refaits en ~9 s.
- Test réel : `WP_TEST_PACKAGE=… WP_TEST_ISO=… WP_TEST_FILES=<fichiers d'origine du paquet> cargo test
  --release makes_a_package_again -- --ignored`.

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

- `type` : `toggle` (valeur 0 ou 1), `choice` (une des `choices`) ou `number` (de `min` à `max`, par `step` :
  boutons − et +, ou la valeur tapée dans son champ, ramenée dans les bornes et sur un pas).
- `riivolution` : le réglage choisit une option d'un des XML du paquet, sans code dans le mod. Pour un
  `choice`, la valeur est le numéro du choix Riivolution (0 désactive l'option) ; un `toggle` active le
  choix `on` (1 par défaut). L'option ne doit pas être aussi dans les `options` fixes du patch.
- `memory` : la valeur est écrite en big-endian sur `size` octets (1, 2 ou 4) à `offset`, avant le
  démarrage du jeu. C'est au code du mod (dans son `main.dol`) de la lire. Le launcher l'écrit par un XML
  Riivolution qu'il génère (`mods/.settings-<id>.xml`).
- Une valeur enregistrée que le paquet ne permet plus (après une mise à jour) revient à `default`.
- Un préréglage est un `choice` dont les choix règlent d'autres réglages d'un coup, sans option
  Riivolution ni `memory` :

  ```json
  { "id": "difficulte", "type": "choice", "label": "Difficulté", "default": 1, "choices": [
      { "value": 0, "label": "Douce", "sets": { "yeti": 25, "mur": 3 } },
      { "value": 1, "label": "Normale", "sets": { "yeti": 50, "mur": 4 } },
      { "value": 2, "label": "Personnalisée" } ] }
  ```

  Sa valeur n'est pas enregistrée : c'est le premier choix dont les `sets` correspondent aux autres
  réglages, sinon le seul choix sans `sets` (personnalisé, proposé seulement quand c'est lui). Changer un
  réglage qu'il règle le fait donc passer à « Personnalisée ». Son `default` doit être le choix que
  donnent les valeurs par défaut.
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
  "library": [],
  "modes": [{ "mode": "board-game-island", "image": "theme/ile-hivernale.png", "name": { "fr": "Île hivernale" } }]
}
```

- `colors` : les variables CSS du launcher, `#rrggbb` (`shadow` et `scrim` : trois nombres). Une couleur
  absente garde sa valeur de `src/app.css`.
- `hero` : ce qui bouge autour du cadre des modes de l'accueil, dans une zone de 550×460 dont le cadre
  occupe à peu près (90, 66)–(482, 372). Il remplace le dé du launcher. `home` et
  `library` : des images posées derrière ces écrans, en coordonnées d'un écran 1280×720. Images SVG ou PNG de
  2 Mo au plus.
- `modes` (facultatif) : l'image d'un mode du jeu (`board-game-island`, `globe-trot`, `mii-of-a-kind`,
  `spin-off`, `bingo`, `friend-connection`, `balance-boat`, `match-up`, `animal-tracker`, `hide-n-hunt`,
  `time-bomb`, `word-bomb`, `buddy-quiz`), en 4:3, à la place de celle de l'ISO, et son nom si `name` est
  donné. Un autre identifiant ajoute un mode de la version : il lui faut un `name`. Ces images passent
  avant les autres modes, dans l'ordre du thème.
- `modesOnly` (facultatif) : `true` pour ne montrer que les `modes` du thème, aucun de ceux du jeu.
  Chacun doit alors avoir un `name`.
- `motion` (facultatif) : `fall`, `float`, `wobble` ou `bob`, les mouvements de `src/app.css`.
- `tagline` (facultatif) : une phrase, ou la même par langue du launcher, `{ "fr": "…", "en": "…", "es": "…" }`.
  Une langue absente prend l'anglais, puis le français.
- Un thème n'est que de la donnée : pas de code ni de HTML, car la page du launcher a accès à ses commandes.
  Les images passent par `<img>`, où un SVG ne peut ni exécuter de script ni charger quoi que ce soit.

Les `.wpver` de `src-tauri/versions/` sont fournis avec le launcher : installés au premier lancement, puis
remplacés seulement si le launcher en apporte une version plus récente. Ils ne peuvent pas être retirés.
Une nouvelle installation démarre sur `prstance.mode-hiver`.

Une version a les mêmes patchs qu'un mod, `code` compris : elle se lie aux mods activés et reste compatible avec eux.
Mode Hiver (`prstance.mode-hiver` 2026.12.2) vient de wp-core `mods/winter` : `python3 tools/wpmod_build.py mods/winter`
en fait un `.wpver` (`"package": "version"` dans son `wpmod.json`), à copier dans `src-tauri/versions/`. Un `.wpver`
fourni doit être versionné (la CI de release l'embarque).

Ancienne recette, sans le système de mods compatibles, depuis wp-winter (branche `mod/winter`) :

1. Dans wp-winter, `WINTER_DEBUG` à 0 (`include/wp/mr001/winter.h`), puis `ninja` (tout, pas seulement
   les RELs modifiés : le mode ajoute du code à `main.dol`, donc chaque REL doit être relié au nouveau).
2. `python3 scripts/winter-settings.py ../../mods/wp-winter/build/SUPP01` (ici) : l'adresse des réglages du mode
   (`gWinterSettings`, dans `main.dol`) change à chaque build ; le script la reporte dans
   `themes/mode-hiver/manifest.json` et vérifie que `main.dol` y a bien les valeurs par défaut.
3. `python3 tools/make_riivolution.py --no-build`. Son paquet ne met pas `main.dol` ni les RELs que le
   projet ne compile pas : il faut les ajouter comme `tools/run_dolphin.py` les installe, `main.dol` en
   `<file disc="main.dol" external="sys/main.dol" />` (Dolphin seulement) et les autres RELs reliés par
   `tools/relink_disc_rels.py`. Sinon le jeu plante dès qu'un module arrive.
4. Sans `LISEZ-MOI.txt` ni `README.txt` (l'installation sur Wii), on en fait la version :

```sh
python3 scripts/make-wpmod.py dist/WinterMode --id prstance.mode-hiver --name "Mode Hiver" \
    --version 2026.11.12 --game SUPP01 --author prstance --conflicts prstance.mode-duo='*' \
    --conflicts prstance.mode-lave='*' --extra ../../mods/wp-winter/mods/winter/CREDITS.md \
    --theme themes/mode-hiver --disc "chemin/Wii Party (Europe).rvz" \
    --out src-tauri/versions/Mode-Hiver-2026.11.12.wpver
```

Le nom, la description par langue et les réglages (difficulté, yéti, sceau, mur, corde, mini-jeux d'hiver,
tempête, tenues, bonnets) viennent de `themes/mode-hiver/manifest.json` (pris d'office avec `--theme`, sauf
si `--manifest` en donne un autre). Le détail de chaque réglage côté jeu est dans wp-winter,
`docs/winter_mode.md` › « Réglages du launcher ».

## Mises à jour du launcher

Paramètres › À propos › « Rechercher une mise à jour » (`src-tauri/src/update.rs`, plugin updater de Tauri) :
le launcher lit `latest.json` de la dernière release de **prstance/wp-launcher** (dépôt public qui ne
contient qu'un README et les releases), télécharge l'installateur de son système, vérifie sa signature,
l'installe et redémarre. Rien n'est vérifié tout seul : seulement quand le joueur clique. Sous Linux,
seule l'AppImage sait se mettre à jour (un `.deb` / `.rpm` passe par le gestionnaire de paquets).

- Signature : la clé publique est dans `tauri.conf.json` (`plugins.updater.pubkey`), la clé privée dans
  `~/.tauri/partylauncher.key`, **à sauvegarder** : sans elle, plus aucune mise à jour ne sera acceptée
  par les launchers déjà installés. `npm run tauri build` en local demande
  `TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/partylauncher.key)"` (ou `--config
  '{"bundle":{"createUpdaterArtifacts":false}}'` pour un build non publié).
- Publier : changer `version` dans `tauri.conf.json`, `Cargo.toml` et `package.json`, puis pousser un tag
  `v<version>` sur le dépôt (privé) du code. `.github/workflows/release.yml` construit les trois systèmes
  et publie la release dans prstance/wp-launcher avec `latest.json`. Secrets du dépôt du code :
  `RELEASES_TOKEN` (un jeton qui peut écrire les releases de prstance/wp-launcher) et
  `TAURI_SIGNING_PRIVATE_KEY` (le contenu de la clé privée). Le cœur des mods et les `.wpver` livrés doivent
  être dans le dépôt du code.

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
pour remplir la fenêtre. Elle reste toujours en 16:9 (`src-tauri/src/window.rs`) : taille fixe, non
redimensionnable à la main, si bien que les gestionnaires en mosaïque (Hyprland, Sway…) l'ouvrent
flottante par-dessus les autres. Agrandir alterne entre 1280×720 (réduit si l'écran est petit) et
95 % de l'écran.
