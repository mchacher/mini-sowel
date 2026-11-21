//! The definition of `motion-light` as `GET /recipes` returns it: Sowel's `RecipeInfo` of
//! `sowel-recipe-motion-light` 1.1.2 (`id`, `name`, `description`, `slots`, `i18n.fr`, `icon`),
//! absent optional fields omitted.

use serde_json::{Value, json};

/// The id of the recipe.
pub const ID: &str = "motion-light";
/// Its `RecipeInfo.name`, what an order's source names.
pub const NAME: &str = "Motion Light";

pub fn info() -> Value {
    json!({
        "id": ID,
        "name": NAME,
        "description": "Turns on lights when motion is detected, turns off after a timeout with no motion. Supports lux threshold, failsafe max-on duration, and button override.",
        "slots": [
            { "id": "zone", "name": "Zone", "description": "Zone to monitor", "type": "zone", "required": true },
            { "id": "lights", "name": "Lights", "description": "Lights to control (must belong to the selected zone)",
              "type": "equipment", "required": true, "list": true, "constraints": { "equipmentType": "light_onoff" } },
            { "id": "timeout", "name": "Timeout", "description": "Delay with no motion before turning off",
              "type": "duration", "required": true, "defaultValue": "10m" },
            { "id": "luxThreshold", "name": "Lux Threshold",
              "description": "Lights won't turn on when ambient brightness exceeds this value; turns off if it rises above threshold + 10% hysteresis",
              "type": "number", "required": false, "constraints": { "min": 0 } },
            { "id": "maxOnDuration", "name": "Safety Auto-off",
              "description": "Failsafe for a stuck sensor: force lights off after this duration with no fresh motion. Active presence keeps resetting it, so it won't cut the light while you're in the room.",
              "type": "duration", "required": false },
            { "id": "buttons", "name": "Manual Switches", "description": "Physical switches for manual on/off toggle",
              "type": "equipment", "required": false, "list": true, "constraints": { "equipmentType": "button" } },
            { "id": "disableWhenDaylight", "name": "Inactive During Day",
              "description": "Do not turn on lights during daytime (based on sunrise/sunset and offsets from settings)",
              "type": "boolean", "required": false }
        ],
        "i18n": {
            "fr": {
                "name": "Lumière sur mouvement",
                "description": "Allume les lumières quand un mouvement est détecté, éteint après un délai sans mouvement. Supporte un seuil de luminosité optionnel et une durée max d'allumage.",
                "slots": {
                    "zone": { "name": "Zone", "description": "Zone à surveiller" },
                    "lights": { "name": "Lumières", "description": "Lumières à contrôler (doivent appartenir à la zone)" },
                    "timeout": { "name": "Délai", "description": "Délai sans mouvement avant extinction" },
                    "luxThreshold": { "name": "Seuil lux (max)", "description": "Au-dessus de ce seuil, les lumières ne s'allument pas" },
                    "maxOnDuration": { "name": "Extinction auto (sécurité)",
                        "description": "Anti-oubli pour capteur bloqué : coupe les lumières après cette durée sans nouveau mouvement. Une présence active la réarme en continu, donc elle ne coupe pas tant que tu es dans la pièce." },
                    "buttons": { "name": "Interrupteurs", "description": "Interrupteurs physiques pour allumer/éteindre manuellement" },
                    "disableWhenDaylight": { "name": "Inactif le jour", "description": "Ne pas allumer pendant la journée (basé sur lever/coucher du soleil)" }
                }
            }
        },
        "icon": "Lightbulb"
    })
}
