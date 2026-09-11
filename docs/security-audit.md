# Audit de sécurité — 11 septembre 2026

**Rapport initial historique.** Plusieurs points ci-dessous ont depuis été corrigés et testés. Le [suivi des corrections](corrections.md) distingue les résultats positifs, les tests encore en échec et le nouveau blocage Defender du contrôleur malgré une analyse statique sans détection.

## Conclusion

**Aucun mécanisme manifestement malveillant n'a été identifié dans le code Winarchy examiné. Cela ne prouve ni l'innocuité du binaire ni un faux positif Defender. La version reste non approuvée pour une utilisation quotidienne.**

Cet audit initial a été effectué par le même assistant qui a écrit le code : ce n'est pas une expertise indépendante ni une certification. Des défauts réels ont été trouvés et corrigés dans les sources. Les correctifs Win32 doivent encore être testés sur Windows dans des conditions approuvées ; ils n'ont pas été exécutés pendant cet audit.

L'alerte `Trojan:Win32/Bearfoos.B!ml` reste non résolue. L'historique Defender confirme une action réussie sur le fichier et un état `IsActive = false` pour cette détection. Ce dernier indicateur signifie que la menace détectée a été traitée, **pas que le logiciel a été déclaré sain**.

## Périmètre et méthode

Point de départ : commit `388e11b`, dépôt propre. Examen des fichiers Rust de `src/`, du build Slint, de l'UI, de la configuration par défaut, du workflow CI et du script de build. Les tests existants ont servi de contexte ; il n'y a pas eu de test d'intrusion sur le bureau Windows.

Vérifications réalisées :

- Lecture des chemins d'exécution sensibles : hooks clavier/souris, commandes, IPC, création de processus, masquage et positionnement des fenêtres, récupération, écritures de fichiers.
- Consultation **en lecture seule** de l'historique Defender.
- Comparaison des empreintes des anciens exécutables avec le rapport initial : identiques.
- `cargo-audit 0.22.2`, installé avec une version verrouillée, sur le `Cargo.lock` du projet.
- Vérification hors ligne des archives Cargo puis de leurs fichiers extraits.
- Vérification de la provenance des paquets, des chemins transitifs des avertissements et de la table d'importation PE de l'ancien daemon.
- Tests Rust et Clippy Linux ; vérification Clippy de toutes les cibles du projet pour `x86_64-pc-windows-gnu`.

Aucune exclusion antivirus, restauration de quarantaine, modification de protection, exécution du daemon ou transmission de binaire/source à un service d'analyse n'a été effectuée. Les accès réseau de l'audit ont servi à obtenir l'outil Cargo, la base publique RustSec et les références publiques des actions GitHub.

## Dépendances et intégrité

Le verrouillage contient 560 paquets, dont le projet et **559 paquets provenant de crates.io**. Le graphe résolu filtré pour Windows contient 337 nœuds, projet inclus : tous les paquets du verrouillage ne sont donc pas des dépendances actives du binaire Windows. La liste complète comporte 102 cibles de type build script ; leurs contenus n'ont pas tous fait l'objet d'un examen de sécurité manuel.

Résultats de la comparaison locale :

- **559/559 archives** présentes et conformes aux sommes SHA-256 du verrouillage.
- **24 913 fichiers sources extraits** identiques aux archives correspondantes.
- Aucun fichier supplémentaire inattendu dans ces répertoires extraits.
- Aucun paquet tiers provenant d'une URL Git arbitraire ou d'un chemin local.

Ces résultats détectent des différences de contenu, **pas une dépendance malveillante publiée avec un checksum valide**. Ils ne vérifient pas l'intégrité du compilateur, de l'OS, des artefacts intermédiaires ou de tous les composants natifs. L'ancien binaire n'a pas été reproduit indépendamment bit à bit.

RustSec : **0 vulnérabilité connue signalée**, avec 4 avertissements de non-maintenance :

| Paquet | Avertissement | Présence observée |
|---|---|---|
| bincode 2.0.1 | RUSTSEC-2025-0141 | Présent au verrouillage, absent du graphe actif observé par `cargo tree` |
| paste 1.0.15 | RUSTSEC-2024-0436 | Transitif de la chaîne image/compilation Slint |
| rustybuzz 0.20.1 | RUSTSEC-2026-0206 | Mise en forme de texte Slint, compilation et exécution |
| ttf-parser 0.25.1 | RUSTSEC-2026-0192 | Polices Slint, compilation et exécution |

Base RustSec : 1 243 avis, commit `b50980aad8b8f14f77e25a97b32dd94bf008b0af`, mise à jour du 9 septembre 2026. Les avertissements ne sont pas ignorés ou masqués. Une migration des dépendances transitives concernées, notamment via Slint, reste à planifier et à tester ; aucun remplacement aveugle n'a été fait.

Preuves conservées : [résultat RustSec](audit/2026-09-11-rustsec.json) et [résultat d'intégrité](audit/2026-09-11-integrity.json).

## Défauts corrigés dans les sources

Les niveaux indiquent un impact potentiel dans les conditions décrites ; aucune exploitation complète de ces défauts n'a été exécutée sur Windows.

### A01 — Résolution ambiguë des exécutables système et élévation non refusée

**Importance : élevée si le programme est lancé avec des droits administrateur depuis un emplacement contrôlable ; sinon risque local.**

`explorer.exe` et `taskkill.exe` étaient lancés par nom. Leur recherche pouvait dépendre du répertoire courant, du répertoire de l'application ou du PATH. Le programme ne refusait pas un lancement élevé. Le filtre `taskkill /IM explorer.exe /F` n'était pas explicitement limité à la session courante.

Corrections `c36362e` et `eec97c5` : contrôle du token avant ouverture des chemins de configuration/log, résolution de chemins absolus via `GetWindowsDirectoryW` / `GetSystemDirectoryW`, refus du daemon et du watchdog avec un token élevé, filtre explicite de session pour l'arrêt d'Explorer. Les applications configurées restent des commandes de confiance choisies par l'utilisateur ; leur lancement n'est pas une sandbox.

### A02 — Jeton du client IPC et réappropriation du nom du pipe

**Importance : élevée conditionnellement pour un ancien client élevé connecté à un faux serveur ; moyenne pour la disponibilité/authenticité.**

Le client utilisait les options de sécurité implicites de `CreateFile`. Un faux serveur de pipe pouvait chercher à obtenir un jeton utilisable pour agir comme le client. Le serveur fermait puis recréait son unique instance à chaque connexion : le nom cessait temporairement d'être réservé, et le drapeau « première instance » n'était plus appliqué après la première connexion.

Correction `0a1f7d3` : le client demande explicitement le niveau `SECURITY_IDENTIFICATION`, pas un jeton permettant d'agir avec ses privilèges ; le serveur conserve le même handle durant sa vie, avec création exclusive et DACL propriétaire. Cela ne supprime pas la possibilité de préoccuper un nom **avant** le démarrage ni ne constitue une authentification cryptographique du serveur.

### A03 — Une commande IPC tronquée pouvait être exécutée

**Importance : moyenne, sûreté du protocole.**

Une fermeture ou erreur de lecture était traitée comme une fin de ligne. Par exemple, une commande sans terminateur pouvait tout de même être acceptée.

Correction `0a1f7d3` : décodeur borné exigeant un terminateur, refus des trames incomplètes, invalides UTF-8 ou trop grandes. Trois tests couvrent les limites, les erreurs de trame et la consommation d'une seule commande. Le client exige également une réponse complète.

### A04 — Récupération armée après les premières opérations sur les fenêtres

**Importance : moyenne, disponibilité du bureau.**

Les premières règles et opérations WM pouvaient déjà masquer des fenêtres avant l'initialisation du watchdog. Une terminaison forcée dans cette période échappait à la récupération. Certains échecs de handshake laissaient aussi des handles ouverts ; un échec tardif de session pouvait ne pas être répercuté dans le code de sortie.

Correction `466dfbc` : handshake avant de toucher les clients ; arrêt d'Explorer séparé, seulement lorsque les surfaces natives sont prêtes ; handles du watchdog gérés par RAII ; propagation des erreurs de démarrage.

### A05 — Informations privées dans les logs

**Importance : faible à moyenne selon le contenu des titres/arguments.**

Les titres de fenêtres étaient journalisés au niveau INFO ; le mode debug pouvait enregistrer la commande de lancement complète, potentiellement porteuse de secrets.

Correction `962f760` : suppression volontaire de ces titres et arguments des messages concernés. Les anciens logs ne sont pas effacés, et les erreurs TOML peuvent encore révéler une ligne de configuration. Les logs restent à traiter comme des données privées.

### A06 — Chaîne CI insuffisamment verrouillée

**Importance : durcissement préventif de la chaîne de distribution.**

Correction `fff011f` : actions GitHub épinglées à des commits obtenus depuis leurs dépôts officiels, permissions du workflow limitées à la lecture du contenu, audit RustSec ajouté. Le script `scripts/audit-registry.py` permet de reproduire la comparaison locale des sources. L'exécution distante de ce workflow n'a pas été vérifiée ici.

## Points encore ouverts

1. **IPC bloquant, risque de déni de service par un client local autorisé.** Les lectures, écritures et `FlushFileBuffers` du serveur ne disposent pas de deadline. Le timeout du CLI ne protège pas le serveur et ne retire pas une commande déjà en file d'attente. Il faut un transport annulable/borné et des tests avec clients lents ou interrompus.
2. **Parcours de fichiers non bornés.** La découverte Start Menu suit récursivement les répertoires ; les jonctions/liens cycliques et très grands arbres doivent être traités. Les lectures de configuration et la file d'événements ne sont pas plafonnées. Il s'agit principalement de disponibilité dans le contexte du même utilisateur.
3. **Identité et durée de vie des objets Windows.** La gestion repose beaucoup sur la valeur numérique des HWND. Le cas d'un handle réutilisé avant traitement d'un événement, les marqueurs de session basés sur PID et le handshake nommé prévisible méritent un durcissement et des tests de concurrence. Ils ne doivent pas être présentés comme une frontière contre un autre processus du même utilisateur.
4. **Tests négatifs Win32 absents.** Refus d'un token élevé, droits réels du pipe avec un deuxième compte, faux serveur, spoofing de handshake, client déconnecté et chemins système en présence d'exécutables homonymes restent à vérifier dans un environnement de test approprié. La vérification Clippy ne remplace pas ces tests.
5. **Dépendances non maintenues**, en particulier les parseurs de polices actifs, et audit complet des licences avant diffusion publique.
6. **Détection Defender non expliquée.** Aucun diagnostic du moteur Microsoft ni analyse dynamique isolée n'est disponible. Aucun lien causal n'a été démontré entre les défauts corrigés et cette détection.

## Ce que montre l'examen du code

- Le hook clavier consulte les touches/modificateurs pour les raccourcis configurés ; il ne conserve pas le texte tapé dans les autres applications et ne l'envoie pas à un serveur. Le texte saisi dans le launcher est son entrée de recherche explicite.
- Aucun téléchargement de charge utile, service persistant, tâche planifiée, changement de shell au registre, injection par `WriteProcessMemory`/`CreateRemoteThread` ou accès à un magasin d'identifiants n'a été trouvé dans le code applicatif examiné.
- L'arrêt de processus est explicite et réservé à la commande/session Explorer ; fermer une application passe par `WM_CLOSE`.
- Le watchdog est un second processus attendu, sans UI, qui attend la fin du daemon et tente de restaurer fenêtres/Explorer. Il ne réinstalle pas Winarchy et ne le rend pas persistant au démarrage de Windows.
- Aucun client/serveur réseau n'est implémenté directement dans le projet. L'import PE de `ws2_32.dll` ne suffit toutefois pas à conclure sur le trafic réel des bibliothèques, de Windows ou des applications lancées. Aucun traçage réseau dynamique n'a été effectué.

## Validation des correctifs et suite

`cargo fmt --check`, Clippy Linux, Clippy Windows et **32 tests indépendants du bureau** passent. Les trois nouveaux tests portent sur le cadrage IPC. Les changements Win32 sont **compilés/vérifiés, pas validés par exécution Windows** pendant cet audit. Les anciens smoke tests ne valent pas validation de ces modifications.

Les anciens exécutables et l'archive candidate ne sont pas mis à jour : ils restent les pièces identifiées par leurs empreintes dans [le rapport initial](security-validation.md), **pas une version corrigée prête à lancer**.

La suite recommandée est de corriger les points de disponibilité prioritaires, faire réexaminer ces changements, puis tester dans un environnement isolé/approuvé. Une soumission à Microsoft pour examen d'un éventuel faux positif nécessite l'accord de l'utilisateur avant envoi du fichier. Ni une signature ni un résultat RustSec vide ne constituent un certificat d'innocuité.

### Reproduction des vérifications sans lancer Winarchy

```sh
cargo install cargo-audit --version 0.22.2 --locked
cargo audit --json
cargo metadata --locked --format-version 1 > /tmp/winarchy-metadata.json
python3 scripts/audit-registry.py /tmp/winarchy-metadata.json
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo clippy --all-targets --locked --target x86_64-pc-windows-gnu -- -D warnings
cargo test --locked
```

Exécuter ces commandes depuis WSL ; la dernière lance les tests Linux, pas les tests opt-in du bureau Windows. Le contrôle d'intégrité nécessite que Cargo ait déjà téléchargé toutes les archives concernées.
