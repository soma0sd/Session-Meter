# 서드파티 라이선스

SessionMeter의 소스 코드는 MIT 라이선스입니다([`LICENSE`](LICENSE) 참고). 배포본에는 아래 서드파티
자산이 각자의 라이선스로 포함·재배포됩니다.

## 폰트

### Noto Sans

- 저작권: The Noto Project Authors (<https://github.com/notofonts/latin-greek-cyrillic>)
- 라이선스: SIL Open Font License, Version 1.1
- 전문: [`src/assets/fonts/Noto-Sans-OFL.txt`](src/assets/fonts/Noto-Sans-OFL.txt)

### Noto Sans KR

- 저작권: The Noto Project Authors
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
permissive 라이선스(MIT / Apache-2.0 / BSD 등)이며 copyleft 라이선스는 포함하지 않습니다. 전체
목록은 [`src-tauri/Cargo.toml`](src-tauri/Cargo.toml)·[`src-tauri/Cargo.lock`](src-tauri/Cargo.lock)과
[`package.json`](package.json)·[`package-lock.json`](package-lock.json)을 참고하십시오.

> 배포 시 의존성별 라이선스 고지를 완전하게 포함하려면 `cargo about`(Rust)이나 `license-checker`(npm)
> 같은 도구로 통합 고지 파일을 생성하는 것을 권장합니다.
