# 서드파티 라이선스

SessionMeter의 소스 코드는 MIT 라이선스입니다([`LICENSE`](LICENSE) 참고). 배포본에는 아래 서드파티
자산이 각자의 라이선스로 포함·재배포됩니다.

## 폰트

### Noto Sans

- 저작권: The Noto Project Authors (<https://github.com/notofonts/latin-greek-cyrillic>)
- 라이선스: SIL Open Font License, Version 1.1
- 전문: [`src/assets/fonts/Noto-Sans-OFL.txt`](src/assets/fonts/Noto-Sans-OFL.txt)

### Noto Sans KR

- 저작권: Google Inc.
- 라이선스: SIL Open Font License, Version 1.1
- 전문: [`src/assets/fonts/Noto-Sans-KR-OFL.txt`](src/assets/fonts/Noto-Sans-KR-OFL.txt)

라틴·한글 폰트는 [Fontsource](https://fontsource.org/)가 패키징한 상류 Google Noto 폰트로 배포됩니다.

### 트레이 숫자 폰트(`tray-digits.ttf`)

- 저작권: The Noto Project Authors(Noto Sans 파생 서브셋)
- 라이선스: SIL Open Font License, Version 1.1
- 전문: [`src-tauri/assets/fonts/OFL.txt`](src-tauri/assets/fonts/OFL.txt)
- 트레이 아이콘의 숫자 렌더링에 사용하는 서브셋 폰트로, 폰트 파일과 같은 위치에 OFL 전문이 동봉되어
  있습니다.

> SIL Open Font License 1.1은 저작권 고지와 라이선스를 함께 포함하고, 폰트를 단독으로 판매하지 않으며,
> 수정본이 예약 폰트명을 사용하지 않는다는 조건에서 폰트의 번들·임베드·재배포를(본 애플리케이션 내
> 포함을 포함하여) 허용합니다.

## 런타임과 라이브러리

이 애플리케이션은 Tauri v2 프레임워크와 다수의 Rust·npm 의존성으로 빌드됩니다. 각 의존성은
개별 라이선스 조건을 따릅니다. 고정된 Rust 의존성 그래프에는 MIT, Apache-2.0, BSD, ISC,
Unicode-3.0, CDLA-Permissive-2.0, OFL-1.1 및 MPL-2.0 구성 요소가 포함됩니다.

- MPL-2.0 구성 요소: `cssparser`, `cssparser-macros`, `dtoa-short`, `option-ext`, `selectors`
- 전체 의존성 잠금 목록: [`src-tauri/Cargo.lock`](src-tauri/Cargo.lock), [`package-lock.json`](package-lock.json)
- 직접 의존성 목록: [`src-tauri/Cargo.toml`](src-tauri/Cargo.toml), [`package.json`](package.json)

MPL-2.0 구성 요소를 수정하거나 재배포할 때는 해당 라이선스의 파일 단위 조건과 원 저작권 고지를
준수해야 합니다. 배포본에는 `LICENSE`, 이 문서 및 번들 폰트의 OFL 전문을 포함합니다. 의존성 그래프가
변경되는 릴리스에서는 `cargo about`(Rust) 또는 `license-checker`(npm)로 패키지별 통합 고지를 다시
생성·검토해야 합니다.
