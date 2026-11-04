//! The backend's messages in the launcher's language, which the page sets
//! (src/lib/i18n.svelte.js). Written in French where they're used, through tr!;
//! TEXTS translates them, and a text missing there stays in French
//! (scripts/check-i18n.mjs lists them).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::RwLock;

static LANG: RwLock<&str> = RwLock::new("fr");

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
    ("« {a} » indique être incompatible avec « {b} ».", "“{a}” says it doesn't work with “{b}”.", "«{a}» indica que es incompatible con «{b}»."),
    ("« {a} » et « {b} » modifient le même code du jeu ({at}).", "“{a}” and “{b}” change the same game code ({at}).", "«{a}» y «{b}» modifican el mismo código del juego ({at})."),
    (" et {n} autre(s)", " and {n} more", " y {n} más"),
    ("« {a} » et « {b} » remplacent le même fichier ({file}) : « {b} » passe en priorité, l’autre mod peut mal fonctionner.", "“{a}” and “{b}” replace the same file ({file}): “{b}” wins, so the other mod may not work properly.", "«{a}» y «{b}» sustituyen el mismo archivo ({file}): «{b}» tiene prioridad y el otro mod puede fallar."),
    ("Tout ce que modifie « {name} » n’a pas pu être analysé : compatibilité inconnue.", "Not everything “{name}” changes could be checked: compatibility unknown.", "No se ha podido analizar todo lo que modifica «{name}»: compatibilidad desconocida."),
    ("Arrête la partie avant de changer les mods installés.", "Stop the game before changing the installed mods.", "Detén la partida antes de cambiar los mods instalados."),
    ("« {name} » est une version : elle se choisit dans l’écran Versions", "“{name}” is a version: choose it on the Versions screen", "«{name}» es una versión: se elige en la pantalla Versiones"),
    ("{file} : {why}.", "{file}: {why}.", "{file}: {why}."),
    ("Choisis tes mods", "Choose your mods", "Elige tus mods"),
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
    ("{xml} n’a pas l’option « {option} » du réglage « {id} »", "{xml} doesn’t have the option “{option}” of the setting “{id}”", "{xml} no tiene la opción «{option}» del ajuste «{id}»"),
    ("Ce mod n’est plus installé.", "This mod isn’t installed anymore.", "Este mod ya no está instalado."),
    ("La valeur {value} n’est pas permise pour « {setting} ».", "The value {value} isn’t allowed for “{setting}”.", "El valor {value} no está permitido para «{setting}»."),
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

/// Until the page says, the saved choice, or the system's language for "auto".
pub fn init(saved: &str) {
    if saved != "auto" {
        return set(saved);
    }
    let system = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|l| !l.is_empty()));
    let code = system.unwrap_or_default().get(..2).unwrap_or("").to_lowercase();
    set(if ["fr", "es"].contains(&code.as_str()) { &code } else { "en" });
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

#[tauri::command]
pub fn locale_set(lang: String) {
    set(&lang);
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
}
