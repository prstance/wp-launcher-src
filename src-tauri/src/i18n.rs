//! The backend's messages in the launcher's language, which the page sets
//! (src/lib/i18n.svelte.js). Written in French where they're used, through tr!;
//! TEXTS translates them, and a text missing there stays in French
//! (scripts/check-i18n.mjs lists them).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::RwLock;

static LANG: RwLock<&str> = RwLock::new("fr");
/// The computer's languages, most wanted first, as the page reads them (navigator.languages).
static SYSTEM: RwLock<Vec<String>> = RwLock::new(Vec::new());
/// The Wii console's languages, by their number in SYSCONF IPL.LNG.
const CONSOLE: [&str; 7] = ["ja", "en", "de", "fr", "es", "it", "nl"];

/// The launcher's language: "fr", "en" or "es".
pub fn lang() -> &'static str {
    *LANG.read().unwrap()
}

/// (French, English, Spanish), one per line.
const TEXTS: &[(&str, &str, &str)] = &[
    ("Dolphin est absent de ce build du launcher.", "Dolphin is missing from this build of the launcher.", "Dolphin no está incluido en esta versión del launcher."),
    ("Ce fichier n’est pas une image de disque lisible.", "This file isn't a readable disc image.", "Este archivo no es una imagen de disco legible."),
    ("Ce n’est pas Wii Party : ce disque contient « {name} » ({id}).", "This isn't Wii Party: this disc contains “{name}” ({id}).", "No es Wii Party: este disco contiene «{name}» ({id})."),
    ("Choisis ton ISO de Wii Party", "Choose your Wii Party ISO", "Elige tu ISO de Wii Party"),
    ("Image de disque Wii", "Wii disc image", "Imagen de disco de Wii"),
    ("La vérification de l’ISO a échoué.", "The ISO check failed.", "No se ha podido comprobar la ISO."),
    ("L’ISO n’est plus à cet endroit. Choisis-le à nouveau.", "The ISO isn't there anymore. Choose it again.", "La ISO ya no está en ese sitio. Vuelve a elegirla."),
    ("Cette copie présente des anomalies : abîmée, ou modifiée par un mod.", "This copy has problems: it's damaged, or a mod changed it.", "Esta copia tiene anomalías: está dañada o la ha modificado un mod."),
    ("Impossible de démarrer Dolphin : {e}", "Couldn't start Dolphin: {e}", "No se ha podido iniciar Dolphin: {e}"),
    ("Dolphin s’est arrêté : {why}", "Dolphin stopped: {why}", "Dolphin se ha detenido: {why}"),
    ("Dolphin s’est arrêté de façon inattendue.", "Dolphin stopped unexpectedly.", "Dolphin se ha detenido de forma inesperada."),
    ("Connecte au moins une Wiimote avant de lancer le jeu.", "Connect at least one Wii Remote before starting the game.", "Conecta al menos un Wiimote antes de iniciar el juego."),
    ("Choisis d’abord ton ISO de Wii Party.", "Choose your Wii Party ISO first.", "Primero elige tu ISO de Wii Party."),
    ("Le jeu est déjà en cours.", "The game is already running.", "El juego ya está en marcha."),
    ("dossier inconnu : {which}", "unknown folder: {which}", "carpeta desconocida: {which}"),
    ("ce paquet utilise le format {format}, que ce launcher ne connaît pas", "this package uses format {format}, which this launcher doesn't know", "este paquete usa el formato {format}, que este launcher no conoce"),
    ("son identifiant « {id} » n’est pas valide", "its ID “{id}” isn't valid", "su identificador «{id}» no es válido"),
    ("il lui manque un nom ou une version", "it's missing a name or a version", "le falta un nombre o una versión"),
    ("il n’indique pas pour quelle version du jeu il est fait", "it doesn't say which version of the game it's made for", "no indica para qué versión del juego está hecho"),
    ("la plage de versions « {range} » n’est pas valide", "the version range “{range}” isn't valid", "el rango de versiones «{range}» no es válido"),
    ("il ne contient aucun patch", "it doesn't contain any patch", "no contiene ningún parche"),
    ("ce launcher ne sait pas encore charger les patchs « {kind} »", "this launcher can't load “{kind}” patches yet", "este launcher todavía no sabe cargar parches «{kind}»"),
    ("le chemin « {xml} » sort du paquet", "the path “{xml}” points outside the package", "la ruta «{xml}» sale del paquete"),
    ("{xml} est introuvable ou illisible", "{xml} is missing or unreadable", "{xml} no se encuentra o no se puede leer"),
    ("{xml} est invalide : {e}", "{xml} is invalid: {e}", "{xml} no es válido: {e}"),
    ("{xml} n’est pas un fichier Riivolution", "{xml} isn't a Riivolution file", "{xml} no es un archivo de Riivolution"),
    ("{xml} n’a pas l’option « {option} » choisie par le paquet", "{xml} doesn't have the option “{option}” the package picks", "{xml} no tiene la opción «{option}» que elige el paquete"),
    ("le paquet n’active aucune option de {xml}", "the package doesn't turn on any option of {xml}", "el paquete no activa ninguna opción de {xml}"),
    ("{xml} désigne un fichier hors du paquet", "{xml} points to a file outside the package", "{xml} apunta a un archivo fuera del paquete"),
    ("son {manifest} est invalide : {e}", "its {manifest} is invalid: {e}", "su {manifest} no es válido: {e}"),
    ("seuls les fichiers .{extension} sont acceptés", "only .{extension} files are accepted", "solo se aceptan archivos .{extension}"),
    ("ce fichier .{extension} est illisible", "this .{extension} file is unreadable", "este archivo .{extension} no se puede leer"),
    ("il manque le {manifest}", "the {manifest} is missing", "falta el {manifest}"),
    ("son {manifest} est illisible", "its {manifest} is unreadable", "su {manifest} no se puede leer"),
    ("ce fichier est abîmé", "this file is damaged", "este archivo está dañado"),
    ("ce fichier contient un chemin dangereux", "this file contains a dangerous path", "este archivo contiene una ruta peligrosa"),
    ("ce paquet est trop gros une fois décompressé", "this package is too big once unpacked", "este paquete es demasiado grande una vez descomprimido"),
    ("« {name} » ne fonctionne pas sur ton ISO ({game}). Il demande : {games}.", "“{name}” doesn't work with your ISO ({game}). It needs: {games}.", "«{name}» no funciona con tu ISO ({game}). Necesita: {games}."),
    ("« {name} » a besoin du mod « {id} », qui n’est pas activé.", "“{name}” needs the mod “{id}”, which isn't turned on.", "«{name}» necesita el mod «{id}», que no está activado."),
    ("« {name} » demande « {other} » en version {range} (installée : {installed}).", "“{name}” needs “{other}” version {range} (installed: {installed}).", "«{name}» necesita «{other}» en la versión {range} (instalada: {installed})."),
    (" et {n} autre(s)", " and {n} more", " y {n} más"),
    ("« {a} » et « {b} » remplacent le même fichier ({file}) : « {b} » passe en priorité, l’autre mod peut mal fonctionner.", "“{a}” and “{b}” replace the same file ({file}): “{b}” wins, so the other mod may not work properly.", "«{a}» y «{b}» sustituyen el mismo archivo ({file}): «{b}» tiene prioridad y el otro mod puede fallar."),
    ("Tout ce que modifie « {name} » n’a pas pu être analysé : compatibilité inconnue.", "Not everything “{name}” changes could be checked: compatibility unknown.", "No se ha podido analizar todo lo que modifica «{name}»: compatibilidad desconocida."),
    ("Arrête la partie avant de changer les mods installés.", "Stop the game before changing the installed mods.", "Detén la partida antes de cambiar los mods instalados."),
    ("« {name} » est une version : elle se choisit dans l’écran Versions", "“{name}” is a version: choose it on the Versions screen", "«{name}» es una versión: se elige en la pantalla Versiones"),
    ("{file} : {why}.", "{file}: {why}.", "{file}: {why}."),
    ("Choisis tes mods", "Choose your mods", "Elige tus mods"),
    ("Connecte au moins une Wiimote : la Chaîne Mii se joue en pointant l’écran.", "Connect at least one Wii Remote: the Mii Channel is played by pointing at the screen.", "Conecta al menos un Wiimote: el Canal Mii se usa apuntando a la pantalla."),
    ("c’est un Mii de 3DS, de Wii U ou de Switch : seuls les Mii de Wii sont acceptés", "it's a 3DS, Wii U or Switch Mii: only Wii Miis are accepted", "es un Mii de 3DS, Wii U o Switch: solo se aceptan Mii de Wii"),
    ("ce n’est pas un fichier Mii de Wii", "it isn't a Wii Mii file", "no es un archivo Mii de Wii"),
    ("ce fichier ne contient aucun Mii valable", "this file doesn't contain any valid Mii", "este archivo no contiene ningún Mii válido"),
    ("Ton image du jeu n’a pas de partition de mise à jour (c’est souvent le cas des .wbfs) : la Chaîne Mii n’y est pas. Utilise un .iso ou un .rvz complet.", "Your game image has no update partition (often the case with .wbfs): the Mii Channel isn't in it. Use a full .iso or .rvz.", "Tu imagen del juego no tiene partición de actualización (suele pasar con los .wbfs): el Canal Mii no está. Usa un .iso o un .rvz completo."),
    ("La bibliothèque est pleine : {max} Mii au plus, comme sur la Wii.", "The library is full: {max} Miis at most, as on the Wii.", "La biblioteca está llena: {max} Mii como máximo, como en la Wii."),
    ("Ce Mii n’existe pas.", "This Mii doesn't exist.", "Este Mii no existe."),
    ("Choisis tes fichiers Mii", "Choose your Mii files", "Elige tus archivos Mii"),
    ("Mii de Wii", "Wii Mii", "Mii de Wii"),
    ("Donne-lui un nom.", "Give it a name.", "Ponle un nombre."),
    ("Mods Wii Party", "Wii Party mods", "Mods de Wii Party"),
    ("il lui manque son thème", "it's missing its theme", "le falta su tema"),
    ("son thème définit une couleur inconnue : « {name} »", "its theme sets an unknown color: “{name}”", "su tema define un color desconocido: «{name}»"),
    ("la couleur « {name} » de son thème n’est pas valide : « {value} »", "the color “{name}” of its theme isn't valid: “{value}”", "el color «{name}» de su tema no es válido: «{value}»"),
    ("son thème demande un mouvement inconnu : « {motion} »", "its theme asks for an unknown motion: “{motion}”", "su tema pide un movimiento desconocido: «{motion}»"),
    ("l’image « {image} » est hors du paquet", "the image “{image}” is outside the package", "la imagen «{image}» está fuera del paquete"),
    ("l’image « {image} » n’est ni un SVG ni un PNG", "the image “{image}” is neither an SVG nor a PNG", "la imagen «{image}» no es ni SVG ni PNG"),
    ("l’image « {image} » est introuvable", "the image “{image}” can't be found", "no se encuentra la imagen «{image}»"),
    ("l’image « {image} » est trop lourde", "the image “{image}” is too big", "la imagen «{image}» pesa demasiado"),
    ("Arrête la partie avant de changer de version.", "Stop the game before switching versions.", "Detén la partida antes de cambiar de versión."),
    ("Cette version n’est plus installée.", "This version isn't installed anymore.", "Esta versión ya no está instalada."),
    ("Choisis une version", "Choose a version", "Elige una versión"),
    ("Versions Wii Party", "Wii Party versions", "Versiones de Wii Party"),
    ("Cette version est fournie avec le launcher.", "This version comes with the launcher.", "Esta versión viene con el launcher."),
    ("déconnectée", "disconnected", "desconectado"),
    ("Service Bluetooth injoignable ({e})", "Can't reach the Bluetooth service ({e})", "No se puede acceder al servicio Bluetooth ({e})"),
    ("adresse invalide : {addr}", "invalid address: {addr}", "dirección no válida: {addr}"),
    ("pas de réponse", "no response", "sin respuesta"),
    ("Démarrage…", "Starting…", "Iniciando…"),
    ("Bluetooth indisponible", "Bluetooth unavailable", "Bluetooth no disponible"),
    ("Gestionnaire de manettes arrêté", "Controller manager stopped", "El gestor de mandos se ha detenido"),
    ("Manette Classique", "Classic Controller", "Mando Clásico"),
    ("Manette Classique Pro", "Classic Controller Pro", "Mando Clásico Pro"),
    ("Guitare", "Guitar", "Guitarra"),
    ("Batterie", "Drums", "Batería"),
    ("Extension", "Extension", "Extensión"),
    ("son réglage « {id} » : {why}", "its setting “{id}”: {why}", "su ajuste «{id}»: {why}"),
    ("identifiant invalide", "invalid id", "identificador no válido"),
    ("type « {kind} » inconnu", "unknown type “{kind}”", "tipo «{kind}» desconocido"),
    ("il manque un texte", "a text is missing", "falta un texto"),
    ("ses choix sont vides ou en double", "its choices are empty or repeated", "sus opciones están vacías o repetidas"),
    ("ses bornes ne vont pas", "its bounds don’t work", "sus límites no son válidos"),
    ("sa valeur par défaut n’est pas permise", "its default value isn’t allowed", "su valor por defecto no está permitido"),
    ("{xml} n’est pas un patch du paquet", "{xml} isn’t one of the package’s patches", "{xml} no es un parche del paquete"),
    ("l’option « {option} » est déjà choisie par le paquet", "the package already picks the option “{option}”", "el paquete ya elige la opción «{option}»"),
    ("un choix Riivolution compte à partir de 1", "Riivolution choices count from 1", "las opciones de Riivolution se cuentan desde 1"),
    ("son emplacement en mémoire n’est pas valide", "its memory location isn’t valid", "su posición en memoria no es válida"),
    ("ses valeurs ne tiennent pas en {size} octet(s)", "its values don’t fit in {size} byte(s)", "sus valores no caben en {size} byte(s)"),
    ("il lui faut soit une option Riivolution (sauf pour un nombre), soit un emplacement en mémoire", "it needs either a Riivolution option (not for a number) or a memory location", "necesita una opción de Riivolution (salvo para un número) o una posición en memoria"),
    ("un autre réglage choisit déjà cette option", "another setting already picks this option", "otro ajuste ya elige esta opción"),
    ("identifiant en double", "duplicate id", "identificador repetido"),
    ("il demande une version plus récente du launcher ({range}) : mets PartyLauncher à jour dans Paramètres › À propos", "it needs a newer launcher ({range}): update PartyLauncher in Settings › About", "necesita una versión más reciente del launcher ({range}): actualiza PartyLauncher en Ajustes › Acerca de"),
    ("Impossible de lire {file} sur ton ISO.", "Couldn’t read {file} from your ISO.", "No se ha podido leer {file} en tu ISO."),
    ("Impossible de lire {file} dans les mods installés.", "Couldn’t read {file} in the installed mods.", "No se ha podido leer {file} en los mods instalados."),
    ("« {name} » a besoin du fichier d’origine {file}, que ton ISO n’a pas : il lui faut le disque d’origine, sans mod.", "“{name}” needs the original {file}, which your ISO doesn’t have: it needs the original disc, without mods.", "«{name}» necesita el archivo original {file}, que tu ISO no tiene: necesita el disco original, sin mods."),
    ("« {name} » est abîmé ({file}) : réinstalle-le.", "“{name}” is damaged ({file}): install it again.", "«{name}» está dañado ({file}): vuelve a instalarlo."),
    ("son delta « {delta} » n’est pas valide", "its delta “{delta}” isn’t valid", "su delta «{delta}» no es válido"),
    ("son delta « {delta} » est introuvable ou abîmé", "its delta “{delta}” is missing or damaged", "su delta «{delta}» no se encuentra o está dañado"),
    ("un préréglage ne choisit rien lui-même : ni option Riivolution, ni emplacement en mémoire", "a preset doesn’t pick anything itself: no Riivolution option, no memory location", "un preajuste no elige nada por sí mismo: ni opción de Riivolution, ni posición en memoria"),
    ("il faut à un préréglage un seul choix sans « sets » (personnalisé)", "a preset needs exactly one choice without “sets” (custom)", "un preajuste necesita una sola opción sin «sets» (personalizado)"),
    ("il règle « {id} » sur {value}, que ce réglage ne permet pas", "it sets “{id}” to {value}, which that setting doesn’t allow", "pone «{id}» en {value}, que ese ajuste no permite"),
    ("sa valeur par défaut n’est pas celle que donnent les autres réglages par défaut", "its default value isn’t the one the other settings’ defaults give", "su valor por defecto no es el que dan los valores por defecto de los demás ajustes"),
    ("{xml} n’a pas l’option « {option} » du réglage « {id} »", "{xml} doesn’t have the option “{option}” of the setting “{id}”", "{xml} no tiene la opción «{option}» del ajuste «{id}»"),
    ("son thème montre un mode sans nom : « {mode} »", "its theme shows a mode without a name: “{mode}”", "su tema muestra un modo sin nombre: «{mode}»"),
    ("Impossible de vérifier les mises à jour : {e}", "Couldn’t check for updates: {e}", "No se han podido buscar actualizaciones: {e}"),
    ("Arrête la partie avant de mettre le launcher à jour.", "Stop the game before updating the launcher.", "Detén la partida antes de actualizar el launcher."),
    ("Le launcher est déjà à jour.", "The launcher is already up to date.", "El launcher ya está actualizado."),
    ("La mise à jour a échoué : {e}", "The update failed: {e}", "La actualización ha fallado: {e}"),
    ("Ce mod n’est plus installé.", "This mod isn’t installed anymore.", "Este mod ya no está instalado."),
    ("Les outils de test ne sont que dans le launcher en développement.", "The test tools are only in the launcher in development.", "Las herramientas de prueba solo están en el launcher en desarrollo."),
    ("La valeur {value} n’est pas permise pour « {setting} ».", "The value {value} isn’t allowed for “{setting}”.", "El valor {value} no está permitido para «{setting}»."),
    ("Dans la Chaîne Mii", "In the Mii Channel", "En el Canal Mii"),
    ("Crée un Mii", "Making a Mii", "Creando un Mii"),
    ("sans mod", "no mods", "sin mods"),
    ("1 mod", "1 mod", "1 mod"),
    ("{n} mods", "{n} mods", "{n} mods"),
    ("Jeu d’origine", "Original game", "Juego original"),
    ("Dans le launcher", "In the launcher", "En el launcher"),
    ("Menu principal", "Main menu", "Menú principal"),
    ("Dans un mini-jeu", "In a minigame", "En un minijuego"),
    ("Mini-jeu : {name}", "Minigame: {name}", "Minijuego: {name}"),
    ("Mini-jeu", "Minigame", "Minijuego"),
    ("son mini-jeu « {id} » n’a pas un numéro valide", "its minigame “{id}” doesn’t have a valid number", "su minijuego «{id}» no tiene un número válido"),
    ("sa variable n’est pas dans le code du paquet", "its variable isn't in the package's code", "su variable no está en el código del paquete"),
    ("un Mii s’écrit dans une variable du code du paquet, sur 8 octets", "a Mii is written into a variable of the package's code, in 8 bytes", "un Mii se escribe en una variable del código del paquete, en 8 bytes"),
    ("seul un Mii s’écrit sur 8 octets", "only a Mii is written in 8 bytes", "solo un Mii se escribe en 8 bytes"),
    ("un de ses patchs désigne un fichier hors du paquet", "one of its patches points to a file outside the package", "uno de sus parches señala un archivo fuera del paquete"),
    ("{file} est introuvable dans le paquet", "{file} can't be found in the package", "{file} no está en el paquete"),
    ("« {b} » remplace en entier un fichier que « {a} » modifie ({file}) : « {b} » doit passer au système de mods compatibles.", "“{b}” replaces a whole file that “{a}” changes ({file}): “{b}” must move to the compatible mods system.", "«{b}» sustituye por completo un archivo que «{a}» modifica ({file}): «{b}» debe pasar al sistema de mods compatibles."),
    ("« {a} » et « {b} » changent le même texte du jeu ({key}) : celui de « {b} » s’affiche.", "“{a}” and “{b}” change the same game text ({key}): the one from “{b}” is shown.", "«{a}» y «{b}» cambian el mismo texto del juego ({key}): se muestra el de «{b}»."),
    ("Ce build du launcher n’a pas le cœur des mods : mets-le à jour.", "This build of the launcher doesn't have the mods' core: update it.", "Esta versión del launcher no tiene el núcleo de los mods: actualízala."),
    ("« {name} » a été fait pour l’ancien cœur des mods : il faut sa nouvelle version.", "“{name}” was made for the old mods' core: it needs its new version.", "«{name}» se hizo para el antiguo núcleo de los mods: necesita su nueva versión."),
    ("Le cœur des mods a besoin du fichier d’origine {file}, que ton ISO n’a pas : il lui faut le disque d’origine de Wii Party (SUPP01), sans mod.", "The mods' core needs the original {file}, which your ISO doesn’t have: it needs the original Wii Party disc (SUPP01), without mods.", "El núcleo de los mods necesita el archivo original {file}, que tu ISO no tiene: necesita el disco original de Wii Party (SUPP01), sin mods."),
    ("Le cœur des mods est abîmé ({file}) : réinstalle le launcher.", "The mods' core is damaged ({file}): reinstall the launcher.", "El núcleo de los mods está dañado ({file}): reinstala el launcher."),
    ("« {a} » et « {b} » sont incompatibles entre eux.", "“{a}” and “{b}” aren't compatible with each other.", "«{a}» y «{b}» son incompatibles entre sí."),
    ("Le code des mods ne s’assemble pas{who} : {why}", "The mods' code doesn't fit together{who}: {why}", "El código de los mods no encaja{who}: {why}"),
    ("son patch de son « {patch} » n’est pas valide", "its sound patch “{patch}” isn't valid", "su parche de sonido «{patch}» no es válido"),
    ("« {a} » et « {b} » changent le même son du jeu ({key}) : celui de « {b} » est gardé.", "“{a}” and “{b}” change the same game sound ({key}): the one from “{b}” is kept.", "«{a}» y «{b}» cambian el mismo sonido del juego ({key}): se queda el de «{b}»."),
    ("Le dossier de Dolphin choisi n’existe plus ({dir}) : choisis-en un autre dans Paramètres › Jeu et émulateur.", "The chosen Dolphin folder doesn't exist anymore ({dir}): pick another one in Settings › Game and emulator.", "La carpeta de Dolphin elegida ya no existe ({dir}): elige otra en Ajustes › Juego y emulador."),
    ("Arrête la partie avant de changer de dossier Dolphin.", "Stop the game before changing the Dolphin folder.", "Detén la partida antes de cambiar la carpeta de Dolphin."),
    ("Ce n’est pas un dossier utilisateur de Dolphin : il n’a pas de dossier Wii. Lance une fois un jeu Wii avec ce Dolphin, ou choisis le dossier qui contient Wii, Config et GameSettings.", "This isn't a Dolphin user folder: it has no Wii folder. Start a Wii game once with this Dolphin, or pick the folder that holds Wii, Config and GameSettings.", "No es una carpeta de usuario de Dolphin: no tiene carpeta Wii. Inicia una vez un juego de Wii con ese Dolphin o elige la carpeta que contiene Wii, Config y GameSettings."),
    ("Choisis le dossier utilisateur de Dolphin", "Choose Dolphin's user folder", "Elige la carpeta de usuario de Dolphin"),
    ("Tour {n}/{max}", "Round {n}/{max}", "Ronda {n}/{max}"),
    ("Tour {n}", "Round {n}", "Ronda {n}"),
    ("J{n}", "P{n}", "J{n}"),
    ("{who} {n} pts", "{who} {n} pts", "{who} {n} pts"),
    ("Cases restantes : {list}", "Spaces left: {list}", "Casillas restantes: {list}"),
    ("Pas de réseau local : connecte l’ordinateur au Wi-Fi ou par câble.", "No local network: connect the computer to Wi-Fi or by cable.", "No hay red local: conecta el ordenador por Wi-Fi o por cable."),
    ("Aucun port libre pour les téléphones ({from}–{to})", "No free port for phones ({from}–{to})", "No hay ningún puerto libre para los teléfonos ({from}–{to})"),
    ("{n} %", "{n}%", "{n} %"),
    ("Téléphone", "Phone", "Teléfono"),
    ("Manettes", "Controllers", "Mandos"),
    ("Une batterie est presque vide.", "A battery is almost empty.", "Una batería está casi vacía."),
    ("Batterie faible", "Low battery", "Batería baja"),
    ("Branche le téléphone de J{n} sur un chargeur.", "Plug P{n}'s phone into a charger.", "Conecta el teléfono de J{n} a un cargador."),
    ("Les piles de la Wiimote de J{n} sont presque vides.", "P{n}'s Wii Remote batteries are almost empty.", "Las pilas del Wiimote de J{n} están casi gastadas."),
    ("le code des mods", "the mods’ code", "el código de los mods"),
    ("Le téléphone de J{n} est déconnecté", "P{n}’s phone is disconnected", "El teléfono de J{n} se ha desconectado"),
    ("La Wiimote de J{n} est déconnectée", "P{n}’s Wii Remote is disconnected", "El Wiimote de J{n} se ha desconectado"),
    ("Rouvre la page du launcher sur le téléphone : il reprendra sa place.", "Open the launcher’s page on the phone again: it will take its place back.", "Vuelve a abrir la página del launcher en el teléfono: recuperará su plaza."),
    ("Appuie sur 1 et 2 de la Wiimote : elle reprendra sa place.", "Press 1 and 2 on the Wii Remote: it will take its place back.", "Pulsa 1 y 2 en el Wiimote: recuperará su plaza."),
    ("J{n} est de retour", "P{n} is back", "J{n} ha vuelto"),
    ("J{n} rejoint la partie", "P{n} joins the game", "J{n} se une a la partida"),
    ("Sensor bar trouvée", "Sensor bar found", "Barra de sensores encontrada"),
    ("Le pointeur suit la sensor bar.", "The pointer follows the sensor bar.", "El puntero sigue la barra de sensores."),
    ("Plus de sensor bar", "No sensor bar anymore", "Ya no hay barra de sensores"),
    ("Le pointeur suit le gyroscope de la manette.", "The pointer follows the controller’s gyroscope.", "El puntero sigue el giroscopio del mando."),
    ("Gyroscope à étalonner", "Gyroscope needs calibrating", "Hay que calibrar el giroscopio"),
    ("Pose la manette à plat sans la toucher un instant : le pointeur se règle.", "Put the controller down flat without touching it for a moment: the pointer sets itself up.", "Deja el mando plano sin tocarlo un momento: el puntero se ajusta."),
    ("Pointeur prêt", "Pointer ready", "Puntero listo"),
    ("Le gyroscope est étalonné.", "The gyroscope is calibrated.", "El giroscopio está calibrado."),
    ("Mods actifs", "Active mods", "Mods activos"),
    ("Avec {names}", "With {names}", "Con {names}"),
    ("Sans autre mod", "No other mods", "Sin otros mods"),
    ("Un mod ne s’applique pas entièrement", "A mod doesn’t fully apply", "Un mod no se aplica del todo"),
    ("Une partie du jeu n’est pas celle que le mod attend : il n’est peut-être pas fait pour cette version du jeu.", "Part of the game isn’t what the mod expects: it may not be made for this version of the game.", "Una parte del juego no es la que el mod espera: quizá no esté hecho para esta versión del juego."),
    ("{names} : une partie du jeu n’est pas celle qu’il attend. Il n’est peut-être pas fait pour cette version du jeu.", "{names}: part of the game isn’t what it expects. It may not be made for this version of the game.", "{names}: una parte del juego no es la que espera. Quizá no esté hecho para esta versión del juego."),
    ("Sauvegarde créée", "Save created", "Partida guardada creada"),
    ("Partie sauvegardée", "Game saved", "Partida guardada"),
    ("Mii enregistré", "Mii saved", "Mii guardado"),
    ("Retour au launcher dans {n} s : le Mii rejoint ta bibliothèque.", "Back to the launcher in {n} s: the Mii joins your library.", "Vuelta al launcher en {n} s: el Mii se une a tu biblioteca."),
    ("Petite pause ?", "Time for a break?", "¿Una pausa?"),
    ("Vous jouez depuis {n} h : buvez un peu d’eau et dégourdissez-vous les jambes.", "You’ve been playing for {n} h: have some water and stretch your legs.", "Lleváis {n} h jugando: bebed un poco de agua y estirad las piernas."),
];

/// The French `fr` in the launcher's language.
pub fn text(fr: &str) -> String {
    let lang = *LANG.read().unwrap();
    let found = TEXTS.iter().find(|(f, ..)| *f == fr);
    match (lang, found) {
        ("en", Some((_, en, _))) => en.to_string(),
        ("es", Some((.., es))) => es.to_string(),
        _ => fr.to_string(),
    }
}

/// `tr!("Dolphin s’est arrêté : {why}", why = e)`: the text in the launcher's
/// language, each {name} replaced by its value.
macro_rules! tr {
    ($fr:literal) => {
        $crate::i18n::text($fr)
    };
    ($fr:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut text = $crate::i18n::text($fr);
        $(text = text.replace(concat!("{", stringify!($name), "}"), &$value.to_string());)+
        text
    }};
}
pub(crate) use tr;

fn set(lang: &str) {
    *LANG.write().unwrap() = match lang {
        "en" => "en",
        "es" => "es",
        _ => "fr",
    };
}

/// The system's language from its environment ("fr_FR.UTF-8" → "fr"), until the page says.
fn env_language() -> String {
    let system = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|l| !l.is_empty()));
    system.unwrap_or_default().get(..2).unwrap_or("").to_lowercase()
}

/// Until the page says, the saved choice, or the system's language for "auto".
pub fn init(saved: &str) {
    if saved != "auto" {
        return set(saved);
    }
    let code = env_language();
    set(if ["fr", "es"].contains(&code.as_str()) { &code } else { "en" });
}

/// The console language closest to the computer's (SYSCONF IPL.LNG), English if the
/// Wii has none of its languages.
pub fn console_language() -> u32 {
    let page = SYSTEM.read().unwrap().clone();
    page.into_iter().chain([env_language()]).find_map(|code| CONSOLE.iter().position(|c| *c == code)).map_or(1, |n| n as u32)
}

/// A text from a package: one, or the same per language ({"fr": …, "en": …}).
/// The page picks its own (local() in src/lib/i18n.svelte.js); Display gives
/// the launcher's language, for the backend's messages.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum Text {
    One(String),
    PerLanguage(BTreeMap<String, String>),
}

impl Default for Text {
    fn default() -> Self {
        Text::One(String::new())
    }
}

impl Text {
    /// Empty, or a language left empty.
    pub fn is_blank(&self) -> bool {
        match self {
            Text::One(text) => text.trim().is_empty(),
            Text::PerLanguage(texts) => texts.is_empty() || texts.values().any(|t| t.trim().is_empty()),
        }
    }
}

impl fmt::Display for Text {
    /// In the launcher's language, else English, else French, else any.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Text::One(text) => f.write_str(text),
            Text::PerLanguage(texts) => {
                let lang = *LANG.read().unwrap();
                let text = [lang, "en", "fr"].iter().find_map(|l| texts.get(*l)).or_else(|| texts.values().next());
                f.write_str(text.map_or("", String::as_str))
            }
        }
    }
}

/// `system`: the computer's languages, as two-letter codes, for the game's language.
#[tauri::command]
pub fn locale_set(lang: String, system: Option<Vec<String>>) {
    set(&lang);
    if let Some(system) = system {
        *SYSTEM.write().unwrap() = system;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_falls_back_to_english_then_french() {
        let text: Text = serde_json::from_str(r#"{"fr":"Neige","en":"Snow"}"#).unwrap();
        assert_eq!(text.to_string(), "Neige");
        let french_only: Text = serde_json::from_str(r#"{"fr":"Neige"}"#).unwrap();
        assert_eq!(french_only.to_string(), "Neige");
        assert_eq!(serde_json::from_str::<Text>(r#""Neige""#).unwrap(), Text::One("Neige".into()));
        assert!(serde_json::from_str::<Text>(r#"{"fr":"Neige","en":" "}"#).unwrap().is_blank());
        assert!(Text::default().is_blank());
    }

    #[test]
    fn the_game_speaks_the_computers_language() {
        *SYSTEM.write().unwrap() = vec!["pt".into(), "de".into(), "en".into()];
        assert_eq!(console_language(), 2);
        *SYSTEM.write().unwrap() = vec!["pt".into()];
        assert!([1, CONSOLE.iter().position(|c| *c == env_language()).unwrap_or(1) as u32].contains(&console_language()));
    }
}
