# 1. Plan de tests du thermostat

Document de test ReqtiCraft, créé pour le corpus maison.

## 2. Tests unitaires

TST_REQ1 : Période d'échantillonnage

Vérifier 6 lectures par minute.

[Covers: DES_REQ1]

TST_REQ2 : Table d'étalonnage

Comparer 16 points à une sonde de référence.

[Covers: DES_REQ2]

TST_REQ3 : Saturation

Envoyer 5 °C et 35 °C, vérifier 7 °C et 30 °C.

[Covers: DES_REQ3]

TST_REQ4 : Seuils du relais

Balayer la température autour de la consigne.

[Covers: DES_REQ4]

## 3. Tests d'intégration

TST_REQ5 : API REST

Lire puis écrire la consigne par l'API.

[Covers: DES_REQ5]

TST_REQ6 : Chien de garde

Bloquer la boucle principale, vérifier le redémarrage.

[Covers: DES_REQ6]

