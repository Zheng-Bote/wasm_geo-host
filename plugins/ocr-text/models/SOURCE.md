# PP-OCRv5 models (PaddleOCR)

Text detection and Latin-script recognition models of PaddleOCR in ONNX format, published by the PaddlePaddle organisation on Hugging Face. Model cards: `license: apache-2.0` ([`LICENSE`](LICENSE), Copyright PaddlePaddle Authors). Retrieved 2026-11-27. Used by `plugins/ocr-text/src/paddle.rs` (`include_bytes!` / `include_str!`, ADR 0009).

| File | Hugging Face repository | Original file | Size (bytes) | SHA-256 |
| :--- | :--- | :--- | --: | :--- |
| `PP-OCRv5_mobile_det.onnx` | <https://huggingface.co/PaddlePaddle/PP-OCRv5_mobile_det_onnx> | `inference.onnx` | 4826518 | `a431985659dc921974177a95adcfbb90fd9e51989a5e04d70d0b75f597b6e61d` |
| `PP-OCRv5_mobile_det.yml` | same | `inference.yml` | 903 | `98069072e1b6b37d727fd9d9f11725faa46d6ea0de012f2ed26caea011c37699` |
| `latin_PP-OCRv5_mobile_rec.onnx` | <https://huggingface.co/PaddlePaddle/latin_PP-OCRv5_mobile_rec_onnx> | `inference.onnx` | 8042023 | `7888113072263cb471b93f66dd5e2ad70548dc526fa1ace760d0d973dd121498` |
| `latin_PP-OCRv5_mobile_rec.yml` | same | `inference.yml` | 6817 | `0bbe984570f597af3638e50bdf2e8276f3ab26a61966096538b3b0d1849f5c84` |

The `.yml` files are the models' own configuration. The plugin reads the character dictionary (836 entries) from `latin_PP-OCRv5_mobile_rec.yml`. The pre- and post-processing parameters follow both files.
