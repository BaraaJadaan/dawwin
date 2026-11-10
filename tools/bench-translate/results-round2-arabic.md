COMET-22 (higher is better), per direction; 150 sentences each; Q4_K_M on a 4-core CPU runner.

| Model | GB | ar-en | en-ar | COMET avg | chrF++ avg | tokens/s | cut off |
|---|---|---|---|---|---|---|---|
| hy-mt2-7b | 4.62 | 80.5 | 79.3 | **79.9** | 42.0 | 5.1 | 0 |
| milmmt46-12b | 7.3 | 80.1 | 78.4 | **79.2** | 41.9 | 3.1 | 0 |
| milmmt46-4b | 2.49 | 79.1 | 78.9 | **79.0** | 41.1 | 9.3 | 0 |
| hy-mt2-1.8b | 1.13 | 77.7 | 77.5 | **77.6** | 39.0 | 18.0 | 0 |
| milmmt46-1b-q8 | 1.07 | 77.3 | 76.9 | **77.1** | 39.2 | 34.8 | 0 |
| milmmt46-1b | 0.81 | 77.1 | 76.6 | **76.8** | 39.1 | 39.5 | 0 |

chrF++ per direction:

| Model | ar-en | en-ar |
|---|---|---|
| hy-mt2-7b | 49.1 | 35.0 |
| milmmt46-12b | 48.9 | 34.9 |
| milmmt46-4b | 48.0 | 34.3 |
| hy-mt2-1.8b | 44.7 | 33.2 |
| milmmt46-1b-q8 | 46.1 | 32.4 |
| milmmt46-1b | 45.7 | 32.5 |
