# Faire examiner le blocage Defender

## Diagnostic disponible

Les essais déjà consignés établissent une quarantaine antivirus, et non un simple avertissement SmartScreen :

- `winarchy.exe` : `Trojan:Win32/Bearfoos.B!ml` sur une ancienne compilation ;
- `winarchyctl.exe` : `Trojan:Win32/Wacatac.F!ml`, y compris après isolation de ses dépendances graphiques ;
- les analyses à la demande sans détection n'ont pas empêché le blocage ultérieur à l'exécution ;
- le programme témoin Rust, compilé avec la même chaîne Windows-GNU, s'est exécuté correctement.

Cela ne prouve ni un faux positif, ni une cause liée au compilateur, à la signature ou à une API particulière. Voir [l'investigation détaillée](controller-investigation.md).

## Prochaine étape : examen Microsoft

1. Ouvrir **Sécurité Windows → Protection contre les virus et menaces → Historique de protection**. Relever le nom de la menace, la date, le chemin et l'action. Ne pas restaurer le fichier ni choisir « Autoriser sur l'appareil » pour cet essai.
2. Ouvrir le portail officiel : <https://www.microsoft.com/en-us/wdsi/filesubmission>. Utiliser le parcours développeur de logiciel, puis indiquer une détection présumée incorrecte ; les intitulés peuvent évoluer.
3. Soumettre, avec ton accord explicite, **l'exécutable exact concerné**, s'il existe encore dans les artefacts de compilation hors quarantaine. Vérifier son SHA-256 sans l'exécuter. Ne pas remplacer cet échantillon par une nouvelle compilation : elle aurait potentiellement une autre empreinte. Si la seule copie est en quarantaine, demander au support Microsoft la marche à suivre plutôt que la restaurer pour lancer Winarchy.
4. Joindre le contexte ci-dessous et conserver l'identifiant du dossier. Le binaire peut contenir des informations issues de la compilation ; vérifier que sa transmission est acceptable. Ne pas joindre automatiquement sources, configuration personnelle ou journaux complets.
5. Attendre la conclusion de l'analyse. Si Microsoft confirme une correction, mettre à jour les renseignements de sécurité Defender, vérifier que la correction concerne bien cet échantillon, puis refaire une analyse normale. Une analyse seule ne valide pas le fonctionnement : reprendre ensuite les tests sur un bureau de test, Explorer conservé et protections actives. Si le blocage revient, arrêter et compléter le dossier.

Aucun fichier n'a été envoyé dans le cadre de cette procédure. La signature éditeur peut être utile pour la distribution future, mais ne garantit pas la disparition d'une détection antivirus. Une compilation MSVC a depuis été testée : voir la section suivante.

## Résultat de la compilation MSVC (11 septembre 2026)

Tous les binaires détectés étaient des compilations croisées MinGW (`x86_64-pc-windows-gnu`) : import de `msvcrt.dll`, aucune section de ressources (ni version, ni manifeste), non signés, jamais vus par Defender. Les deux détections portent le suffixe `!ml`, c'est-à-dire un verdict du classifieur machine learning, pas une signature.

Sans droits administrateur côté Windows, la solution retenue passe entièrement par WSL : cible `x86_64-pc-windows-msvc` via cargo-xwin, avec clang-cl, lld-link et llvm-rc extraits des paquets Debian dans un préfixe utilisateur, et intégration d'informations de version et d'un manifeste `asInvoker` par `build.rs`. La procédure est dans [installation.md](installation.md) et les empreintes dans [controller-investigation.md](controller-investigation.md).

Sur le poste de développement, cette compilation a passé l'analyse à la demande, puis le contrôleur et le daemon ont démarré sans détection ni blocage, protections actives et sans exclusion. Cela montre que la classification tenait aux artefacts MinGW et non à un comportement observé, mais ne constitue pas une décision de faux positif par Microsoft. La soumission ci-dessous reste utile pour les anciennes empreintes.

## Signalement prérempli pour le contrôleur

Les données suivantes concernent **l'essai historique du 11 septembre 2026**, pas une nouvelle observation. Vérifier leur correspondance avec le fichier soumis. Pour une autre détection, adapter les données ; ne pas utiliser l'empreinte du contrôleur pour le daemon.

```text
Subject: Suspected false positive — Winarchy controller — Trojan:Win32/Wacatac.F!ml

I am the developer of Winarchy, a personal Rust Windows window manager.
Please review the attached controller executable for a possible incorrect detection.
I do not have evidence establishing that this is a false positive.

File: winarchyctl.exe
SHA-256: a8be20ed9266e2e331f024c650b702b3bdc155a6fa18c56ad058a1ff1ad2c2c2
Source revision: 62ead81
Build: Rust 1.96.0, x86_64-pc-windows-gnu, release, stripped, unsigned
Detection: Trojan:Win32/Wacatac.F!ml
Threat ID: 2147749375
Recorded detection time: 2026-09-11 18:25:23 (host local time)
Security intelligence version recorded: 1.459.156.0
Engine version recorded: 1.1.26080.3

The controller sends bounded commands to a local named pipe owned by the user.
It has been separated from the daemon's GUI dependencies; this controller
has no user32 import. An on-demand directory scan reported no threats,
but the attempted launch was blocked and the file was quarantined.
We cannot establish whether its main function was reached.

A separate print-only Rust control built with the same toolchain ran successfully.
Code Integrity 3076 and ASR 1122 audit events also occurred for that control;
these audit events do not explain the separate antivirus quarantine.

No antivirus exclusions or protection disablement were used.
Please confirm the classification and advise whether a detection correction
or further evidence is needed.
```

Pour vérifier un artefact conservé sous Linux/WSL, sans le lancer :

```bash
sha256sum /chemin/vers/winarchyctl.exe
```

Le dossier technique et les limites des essais sont dans [controller-investigation.md](controller-investigation.md). Ne pas multiplier les variantes de binaire jusqu'à obtenir une analyse favorable : cela ne fournit ni explication ni validation de sécurité.
