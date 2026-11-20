# Mini-Sowel, note de cadrage

Nov 20, 2026, @Marc CHACHEREAU

## Résumé exécutif

Le mini-Sowel est un Sowel minimal dans un boîtier Zigbee du commerce à environ 43 $ : autonome pour une annexe ou un petit logement, et complémentaire du grand Sowel quand il y en a un. Le détail suit ce résumé.

**Pour quoi.** Les annexes d'une maison (garage, pool house, atelier, dépendance, abri de jardin), souvent hors de portée du réseau Zigbee principal, ainsi que les résidences secondaires et les petits logements.

- **Autonome** : il tourne seul, sans serveur, sans Docker et sans cloud.
- **Complémentaire** : relié à un Sowel, ses équipements apparaissent dans celui-ci, et il continue de fonctionner si le lien tombe.

**Ce qu'il garde de Sowel.** Devices, équipements, zones, recettes et modes, avec les mêmes types et le même format d'API. L'énergie, les intégrations cloud et l'historique long restent dans le grand Sowel.

**Ce qui le distingue.**

- Il **propose lui-même les recettes** : on appaire un capteur, on choisit la pièce, il suggère « Allumer le garage quand quelqu'un entre ? ».
- Il reconnaît une **grande partie des devices de Zigbee2MQTT**, grâce à un export automatique de ses convertisseurs.
- Ses **recettes se téléchargent** comme dans Sowel, en WebAssembly, dans un vrai bac à sable.

**Comment.** Un ESP32 fait tourner le logiciel, écrit en Rust ; une radio EFR32MG24 gère le Zigbee avec une pile éprouvée. Le matériel est une carte du commerce (Sonoff Dongle Max), dont on remplace seulement le firmware. Les mises à jour arrivent par Internet, signées, avec retour automatique en cas d'échec. Le même code tourne sur le Mac : 90 % du développement se fait sans le boîtier.

**Risques principaux.** Le protocole radio (EZSP) est à écrire en Rust ; la part des convertisseurs Zigbee2MQTT exportables reste à mesurer ; la mémoire de l'ESP32 est limitée.

**Prochaine étape.** Depuis le Mac, faire rejoindre un vrai capteur via le Dongle Max, et mesurer la couverture Zigbee2MQTT. Décision attendue : projet personnel ou produit visé dès le départ.

## Objectif

Le mini-Sowel est un Sowel minimal qui tient dans un boîtier Zigbee du commerce à moins de 50 €, sans serveur, sans Docker et sans cloud. Il garde les concepts de Sowel (devices, équipements, zones, recettes, modes) et vise d'abord les annexes d'une maison (garage, pool house, atelier, dépendance) et les petits logements. Il est autonome quand il est seul, et complémentaire quand un Sowel existe : il en devient alors une extension.

Ce qui le distingue : **la maison se configure toute seule**. On appaire un capteur, on choisit la pièce, et le boîtier propose lui-même les recettes qui s'appliquent. L'utilisateur accepte des propositions au lieu de construire des règles.

C'est un projet séparé du dépôt Sowel, comme le showroom. Ce dont il a besoin côté Sowel devient une issue ordinaire, défendue sur ses propres mérites.

## Périmètre

On garde le modèle de Sowel et on retire tout ce qui demande un serveur.

| Sowel | Mini-Sowel |
| --- | --- |
| Devices découverts par des plugins d'intégration | Devices Zigbee uniquement, définitions générées depuis les convertisseurs de Zigbee2MQTT (voir Fonctions clés) |
| Équipements typés | Mêmes types et mêmes clés de données que Sowel, limités à lumière, volet, capteur, prise, thermostat, bouton |
| Zones et agrégations | Identiques (OU pour le mouvement, moyenne pour la température), arbre peu profond |
| Recettes en plugins JavaScript | Recettes en WebAssembly, téléchargeables et signées (voir Fonctions clés) |
| Modes | Identiques |
| InfluxDB | Historique court (24 h, basse résolution) dans le boîtier, export MQTT optionnel |
| Plugins cloud (Panasonic, Renault, Tuya cloud…) | Absents |
| Énergie, arbitre, finances | Absents, restent dans le grand Sowel |

Une règle : tout ce qui est exposé reprend les noms, les types et le JSON de l'API Sowel. C'est ce qui permet de relier les deux plus tard.

## Architecture

Deux puces : un ESP32 fait tourner tout le logiciel, une radio EFR32MG24 fait tourner la pile Zigbee. Elles se parlent en EZSP sur un port série.

```
┌──────────────────────────── ESP32 (Rust) ────────────────────────────┐
│                                                                      │
│   Cœur Sowel (sans dépendance matérielle)                            │
│   devices, équipements, zones, modes, recettes WebAssembly,          │
│   propositions de recettes                                           │
│        │            │             │              │                   │
│   Adaptateur    Adaptateur    Adaptateur     Adaptateur              │
│   Radio (EZSP)  Stockage      HTTP / WS      OTA                     │
│        │                          │                                  │
└────────┼──────────────────────────┼──────────────────────────────────┘
         │ UART (EZSP)              │ Ethernet PoE / Wi-Fi
┌────────┴─────────┐          ┌─────┴──────────────────────────┐
│ EFR32MG24        │          │ Mini PWA, sowel-plugin-mini,   │
│ EmberZNet (NCP)  │          │ mises à jour signées           │
└────────┬─────────┘          └────────────────────────────────┘
         │ Zigbee
   capteurs, lumières, prises, volets
```

Le cœur ne touche le matériel qu'à travers quatre adaptateurs ; c'est ce qui permet de développer sur le Mac.

- **Radio à part** : EmberZNet est la pile de Zigbee2MQTT et de ZHA, éprouvée sur des millions de foyers. Elle se met à jour séparément, le MG24 sait aussi faire du Thread, et EZSP existe sur des dizaines de produits, donc pas de dépendance à un fournisseur.
- **Rust** : EZSP s'écrit en Rust pur, sans C imposé. Les enums collent aux événements typés de Sowel, `serde` au JSON de son API, et la mémoire est sûre sur un firmware qui tourne des mois.
- **Réseau** : Ethernet d'abord (idéalement PoE), Wi-Fi en secours, pour ne pas disputer la bande 2,4 GHz au Zigbee.

## Fonctions clés

**Propositions de recettes.** Les paramètres d'une recette sont typés (un capteur de mouvement, une lumière) et les équipements sont rangés par pièce. Le boîtier croise les deux et propose les combinaisons valides : « Allumer le salon quand quelqu'un entre ? ». Un tap crée l'instance. Le moteur de propositions peut d'abord être prototypé dans le Sowel actuel, où il serait aussi une vraie fonction.

**Recettes téléchargeables.** Chaque recette est écrite en Rust et compilée en un fichier `.wasm` d'une dizaine de Ko. Le boîtier la télécharge depuis un registre, vérifie sa signature, et l'exécute dans un moteur WebAssembly embarqué (`wasmi`). Une recette n'accède qu'à ce qu'on lui donne : lire des données, envoyer des ordres, programmer des minuteries. C'est un vrai bac à sable, plus strict que l'isolation des plugins Sowel.

Premières recettes à porter : `motion-light`, `motion-light-dimmable`, `switch-light`, `schedule-on-off`, `presence-thermostat`, `vmc-humidity`. Mêmes noms, mêmes paramètres, même comportement que dans Sowel.

**Devices Zigbee2MQTT générés automatiquement.** Les convertisseurs de Zigbee2MQTT (`zigbee-herdsman-converters`, licence MIT, plus de 4000 devices) sont du code TypeScript, mais une grande partie est déclarative : extensions modernes (`onOff`, `temperature`, `battery`…) et tables de datapoints Tuya. Un outil Node lancé en CI charge cette bibliothèque, exporte chaque définition reconnue (empreinte du device, données exposées, correspondance des clusters ou des datapoints) dans un fichier compact, le signe et le publie comme les recettes. Le boîtier l'interprète sans exécuter de JavaScript. Les convertisseurs écrits en code libre restent hors couverture ; leur part est à mesurer dès la première étape.

**Lien avec Sowel.** Un plugin `sowel-plugin-mini` se connecte à l'API du mini-Sowel, et ses équipements apparaissent dans le grand Sowel. Usages : résidence secondaire, maison d'un proche, bâtiment loin du réseau Zigbee. Le mini continue de tourner seul si le lien tombe.

**Pont Matter, plus tard.** Le mini-Sowel expose ses équipements et ses zones comme devices Matter, pour Apple Maison et Google Home. Les zones deviennent des capteurs virtuels (« Salon occupé »). Ce n'est pas dans la V1.

## Interface

Une web app de 100 à 170 Ko gzip, servie par le boîtier, soit environ 10 fois moins que l'UI actuelle de Sowel (1,35 Mo gzip mesuré le 2026-11-08).

- **Pile** : Preact avec `preact/compat` (réutilise des composants React de Sowel et Zustand), une quarantaine d'icônes Lucide en imports nommés, uPlot ou des sparklines SVG pour les courbes, polices système, langue active seule.
- **Service** : assets embarqués dans le firmware (mis à jour avec l'OTA), pré-compressés, noms hachés, `Cache-Control: immutable`. Envoi par blocs depuis la flash, nombre de clients WebSocket limité (par exemple 4).
- **Mini PWA** : une web app avec manifeste et icônes, qui s'ajoute à l'écran d'accueil. Sans HTTPS (`http://192.168.x.x`), pas de service worker : le hors-ligne est perdu, ce qui ne gêne pas puisque l'appli ne sert à rien sans le boîtier, mais les notifications push aussi. Sur Android, l'icône ouvre un raccourci vers le navigateur ; le plein écran sur iPhone reste à vérifier.
  - **Relié à un Sowel** : la page du mini passe par le Sowel, déjà en HTTPS (`app.sowel.org`). On retrouve une vraie PWA avec les notifications, sans rien ajouter au boîtier.

La mesure de l'UI Sowel a donné l'issue [#1187](https://github.com/mchacher/sowel/issues/1187) (découpage du bundle).

## Sécurité et mises à jour

Les mises à jour passent par Internet comme dans Sowel, mais avec un binaire signé au lieu d'une image Docker, et un retour automatique à l'ancienne version si la nouvelle ne démarre pas.

1. Le boîtier lit un manifeste (version, URL, sha256, notes) et l'UI annonce la mise à jour.
2. Le binaire est écrit dans la partition inactive, vérifié (sha256 puis signature), puis le boîtier redémarre dessus.
3. Un contrôle de santé valide la version ; sinon le bootloader revient à l'ancienne. Une coupure de courant ne casse rien.

Trois choses se mettent à jour séparément, chacune avec sa propre clé :

| Quoi | Mécanisme | Clé |
| --- | --- | --- |
| Firmware de l'ESP32 | OTA sur deux partitions, secure boot | RSA-3072, dans un KMS cloud, signée par la CI sur tag avec approbation |
| Firmware de la radio EFR32 | Fichier `.gbl` poussé au bootloader Gecko par l'UART, vérifié par l'ESP32 avant | ECDSA P-256, KMS |
| Recettes et définitions de devices | Fichiers signés, sans redémarrage | Ed25519, KMS |

**Clés de secours.** L'ESP32-S3 grave jusqu'à trois empreintes de clés, révocables. Clé 1 dans le KMS, clés 2 et 3 générées hors ligne et rangées séparément. Si la clé 1 est perdue, une release signée par la clé 2 la révoque. L'ESP32 classique n'a qu'un emplacement : pas de secours possible.

**Usine sans secret.** On livre des images déjà signées. Au premier démarrage, la puce grave les empreintes, génère sa propre clé de chiffrement de flash et coupe JTAG et le téléchargement UART. Condition : recevoir des cartes aux eFuses vierges.

**Conformité.** L'EN 18031 (RED, obligatoire depuis le 1er août 2025) impose notamment aucun mot de passe par défaut commun : l'utilisateur crée le sien à la mise en service, confirmée par un appui sur le bouton. Le CRA ajoute la déclaration des vulnérabilités (depuis septembre 2026) et l'ensemble des exigences au 11 décembre 2027, dont le SBOM, que Cargo produit facilement.

## Matériel

Le mini-Sowel tourne sur un coordinateur Zigbee du commerce à environ 43 $ : on ne conçoit pas de carte, on remplace seulement son firmware.

| Carte | ESP32 | Radio | Mémoire | Connexions |
| --- | --- | --- | --- | --- |
| [SONOFF Dongle Max (Dongle-M)](https://www.cnx-software.com/2025/09/17/sonoff-dongle-max-dongle-m-poe-based-zigbee-thread-gateway-esp32-mg24/) | ESP32-D0WDR2-V3 | EFR32MG24 | 16 Mo flash, 2 Mo PSRAM | Ethernet PoE, Wi-Fi, USB |

Elle reprend exactement l'architecture visée. Son firmware d'origine expose la radio en TCP, ce qui permet de développer depuis le Mac avant de flasher quoi que ce soit.

**Plus tard, seulement si un produit est visé.** Une carte à ESP32-S3, comme la [SMLIGHT SLZB-06MG24U](https://smlight.tech/products/slzb-06mg24u), apporterait des clés de secours et une meilleure protection matérielle ; SMLIGHT et Sonoff font de l'OEM. Ce n'est pas l'objet de cette note.

## Développement et tests

Le même code Rust tourne sur le Mac et sur le boîtier : environ 90 % du travail se fait sur le Mac, avec un dongle USB à 30 €.

1. **Sans matériel** : une fausse radio rejoue des scénarios (capteur qui rejoint, températures, mouvement). On teste le moteur, l'API, l'UI et les propositions.
2. **Vrai Zigbee sans ESP32** : un dongle USB EFR32 parle le même EZSP que le boîtier, ou le Dongle Max avec son firmware d'origine expose sa radio en TCP.
3. **Firmware émulé** : le QEMU d'Espressif ou Wokwi vérifient le démarrage, les partitions et la logique OTA.
4. **Sur le boîtier** : intégration finale, secure boot et eFuses (sur une carte sacrifiée), mesures réelles.

**Garde-fous.** Le Mac cache les limites de l'ESP32. Les tests comptent la mémoire allouée et échouent au-delà d'un plafond (par exemple 200 Ko de pic pour 50 devices). La CI compile pour la cible, suit la taille du binaire et démarre le firmware sous QEMU.

## Risques et inconnues

| Risque | Pourquoi | Parade |
| --- | --- | --- |
| Support des devices | Les convertisseurs de Zigbee2MQTT sont du code, pas des données ; seule leur partie déclarative s'exporte | Export automatique en CI, couverture mesurée dès l'étape 1, définitions écrites à la main pour les devices manquants les plus courants |
| EZSP en Rust | Aucune implémentation Rust éprouvée connue, protocole à écrire | C'est le premier spike, avant tout le reste |
| Mémoire de l'ESP32 | Moteur, UI, TLS et recettes WebAssembly dans 2 Mo de PSRAM | Plafonds mémoire dans les tests dès le début |
| Pertinence des propositions | Des propositions absurdes tuent l'effet magique | Peu de recettes, très bien choisies, testées d'abord sur la prod Sowel |
| HTTPS en local | Pas de PWA ni d'installation sur `http://` | Mini PWA sans service worker en V1, vraie PWA via le Sowel relié |
| Certification Matter | Adhésion CSA, laboratoire, certificat par boîtier : plusieurs milliers d'euros | Hors V1 |
| Clé de signature perdue | Plus aucune mise à jour possible | Clés de secours (ESP32-S3 uniquement) |

À vérifier : la maturité de `rs-matter` pour un pont, le nombre de signatures que le bootloader S3 grave au premier démarrage.

## Étapes

Chaque étape se termine par une démonstration ; on ne passe à la suivante que si elle est réussie.

1. **Spike radio** : depuis le Mac, en Rust, former un réseau Zigbee via le Dongle Max, faire rejoindre un capteur et lire sa température. En parallèle, mesurer la part des convertisseurs Zigbee2MQTT exportables automatiquement.
2. **Moteur sur Mac** : devices, équipements, zones, modes, API au format Sowel, recette `motion-light` qui allume une vraie ampoule.
3. **Propositions** : prototype du moteur de propositions dans le Sowel actuel, jugé sur la prod.
4. **Sur le boîtier** : le même moteur flashé sur le Dongle Max, avec l'UI légère et l'OTA avec retour arrière.
5. **Recettes WebAssembly** : `motion-light` en `.wasm`, téléchargée et exécutée sur le boîtier.
6. **Lien avec Sowel** : plugin `sowel-plugin-mini`.
7. **Produit** : plus tard et hors de cette note, carte à ESP32-S3, secure boot, clés, OEM, conformité.

Décision à prendre avant l'étape 1 : projet personnel ou produit visé dès le départ. La réponse change l'ordre des étapes 4 à 7.
