# Morrow · 明隙

[简体中文（默认）](../../README.md) · [English](../../docs/readme/README.en.md) · [Русский](../../docs/readme/README.ru.md) · [Français](../../docs/readme/README.fr.md) · [Deutsch](../../docs/readme/README.de.md) · [Español](../../docs/readme/README.es.md) · [日本語](../../docs/readme/README.ja.md) · **한국어** · [Português](../../docs/readme/README.pt.md)

내일의 아이디어를 위한 작은 여백.

Morrow(明隙)는 카드를 중심으로 아이디어를 정리하는 로컬 우선 작업 공간이며, 모든 플랫폼을 위한 플러그인 구조로 발전하고 있습니다. 이전 이름은 daemon, 코드 패키지 이름은 `morrow_studio`입니다. Windows에서는 Flutter가 화면을, Rust 호스트와 격리된 Wasm 플러그인이 작업 로직을 담당합니다. Web과 Android는 기존 구현을 사용하며, 모든 플랫폼의 플러그인 검증은 아직 완료되지 않았습니다.

## 다운로드와 호환성

현재 버전은 **0.1.9-test.55+59**이며, **Windows x64 테스트 미리 보기**로 안정 버전이 아닙니다. Windows ZIP, 해당 소스 ZIP, SHA-256 목록을 다운로드하세요. 전체 압축을 풀고 `morrow_studio.exe`를 실행하며, DLL, 호스트, `data`, `plugins`, 라이선스 파일을 모두 유지하세요.

[test.55 다운로드](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.55) · [호환 테스트 버전 test.1](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.1)

`test.1`은 기존 데이터 형식과 호환되는 마지막 `0.1.x` 테스트 버전입니다. 이후 test 버전은 재작성을 진행하므로 호환성을 깨는 변경이 있을 수 있습니다. 아키텍처와 데이터 모델을 확정하고 검증한 뒤 `0.2.0`을 출시합니다. test.1 데이터는 자동으로 가져오거나 덮어쓰지 않습니다. 업그레이드 전에 라이브러리와 원본 보호 파일을 백업하세요. 보호는 Windows 사용자에 연결되므로 데이터베이스 복사만으로 다른 계정으로 이전할 수 없습니다.

## 주요 기능

- 카드와 콘텐츠: 아이디어, 프로젝트, 실험, 즐겨찾기, 검색, 체크리스트, 삭제 취소. Markdown 편집과 미리 보기, 첨부 파일의 독립된 사본을 지원합니다.
- 서식 있는 콘텐츠: 텍스트, 스크린샷, 파일, Office 서식과 표. 복잡하거나 비공개 형식은 미리 보기 또는 첨부 파일로 저장될 수 있으며, 원래 배치를 완전히 재현한다고 보장하지 않습니다.
- 외관: 반투명 유리, 고투명 유리, 액체 유리. 기본·단색·질감·투명 배경, 테마 색상환, 카드와 구성 요소별 설정 및 동작 줄이기를 지원합니다.
- 미디어: 이미지·GIF·동영상 배경, 로컬 음악, 가사, 떠 있는 도움말. 형식 지원은 플랫폼과 디코더에 따라 다릅니다. Web의 투명 배경은 바탕 화면이 아닌 호스트 페이지를 보여 줍니다.
- 배치와 언어: 반응형 콘텐츠와 별도 설정 페이지. 중국어, 영어, 러시아어, 프랑스어, 독일어, 스페인어, 일본어, 한국어, 포르투갈어.
- Windows 데이터 보호: Rust의 통합 저장, 감사 기록 봉인, 라이브러리 스냅샷, 원래 식별 정보의 백업과 복구, 같은 식별 정보의 동시 사용 제한.

## 이번 최적화와 검증

test.55에는 일곱 가지 화면 스타일과 깊이 조절이 추가되고, 컨트롤 애니메이션과 구성 요소 간 간격이 개선되었습니다. 설정 저장과 글꼴 적용 문제를 수정하고, 안전한 백그라운드 종료를 개선했으며, 초안 복구의 기반을 마련했습니다. 정식 UI의 자동 저장은 아직 완료되지 않았습니다.

### 이전 test.54 최적화와 검증

test.54는 라이브러리를 열 때 중복되는 전체 검증을 줄이고, 증거 디코딩을 제한된 병렬 작업으로 처리하며, 같은 검증 트랜잭션 안에서 검증된 크기와 아카이브 다이제스트를 재사용합니다. 매번 전체 검증은 유지합니다. 저장 형식과 내장 플러그인은 바뀌지 않으며, 실행 간 검증 캐시를 저장하지 않습니다.

마지막 읽기 최적화를 이전 병렬 버전과 각각 4회 번갈아 실행해 비교했습니다. 같은 PC와 약 100 MB 라이브러리 사본에서 Dart 진입부터 로딩 화면이 사라질 때까지 중앙값이 2.222초에서 1.299초로 줄었습니다. 사본 등록이 파일 캐시를 예열할 수 있으므로 캐시를 비운 콜드 스타트 보장은 아닙니다. Core 568개, Audit 97개, 실제 호스트 통합 2개가 통과했으며 기존 Core 테스트 1개는 제외됐습니다. 자세한 조건은 [test.54 릴리스 문서](../../reports/0.1.9-test.54-release.md)를 참고하세요.

## 소스에서 실행

Flutter 3.44 / Dart 3.12 또는 호환 버전이 필요합니다. Windows에는 Visual Studio C++ 데스크톱 빌드 도구, Windows SDK, Rust, PATH에 등록된 Cap’n Proto 컴파일러도 필요합니다.

Windows Rust 작업 공간의 전체 빌드, 통합 검사와 패키징:

```powershell
flutter pub get
rustup target add wasm32-unknown-unknown
pwsh -File tool/build_rust_workbench_windows.ps1
```

기존 산출물은 덮어쓸 수 없습니다. 새 버전 또는 새 출력 디렉터리를 사용하세요. `-RefreshArtifact`도 덮어쓰기를 허용하지 않습니다. 배포할 때는 전체 실행 디렉터리를 포함해야 합니다. Web/Android의 빌드와 검증 범위는 플랫폼 문서를 따르며, Windows 빌드로 대신할 수 없습니다.

개발과 기본 검사:

```powershell
flutter run -d windows
flutter run -d chrome
flutter analyze
flutter test
flutter build web --no-web-resources-cdn
```

## 아키텍처, SDK와 다음 과제

목표 구조는 Flutter/Dart UI, 이식 가능한 Rust 코어, 교체 가능한 플러그인 실행 기반입니다. 실행 경계는 고정 계약을, 저장과 자체 교환은 Protobuf + LZ4를 사용합니다. C/C++/Rust SDK와 선언형 플러그인 UI를 개발 중입니다. TS/JS 플러그인은 지원하지 않으며, 동적 Dart 플러그인을 요구하지 않습니다.

전체 SDK는 아직 동결되지 않았습니다. 관리되는 HTTP/HTTPS 요청, 제한적인 API 서비스 노드, TLS 식별 정보 관리가 연결되어 있습니다. 재시작 후 Unknown 결과 대조, 완전한 파일 시스템, 세 언어의 IO SDK와 플랫폼 검증은 남아 있습니다. 라이브러리를 열 때 여전히 전체 이력을 검사합니다. 읽기와 계산을 겹치는 전체 파이프라인, 이력 계층화, 자동 정리는 구현되지 않았습니다.

## 문서

- [릴리스 설명과 검증](../../reports/0.1.9-test.55-release.md)
- [개발 보드와 다음 과제](../../docs/DEVELOPMENT_BOARD.md)
- [아키텍처 로드맵](../../docs/FUTURE_ROADMAP.md)
- [기능 이전과 용량 제한](../../docs/TEST1_RUST_PARITY.md)
- [C/C++/Rust SDK](../../sdk/README.md)
- [플러그인 UI 설계](../../docs/PLUGIN_SDK_AND_UI.md)
- [콘텐츠 캡처와 형식](../../docs/RICH_CAPTURE.md)
- [Android 빌드](../../docs/ANDROID.md)
- [이름 변경 호환성](../../docs/RENAMING.md)
- [이전 README와 단계 기록](../../docs/history/README-before-test54.md)

상세 설계와 검증 보고서는 주로 중국어입니다. 각 README는 같은 현재 버전을 안내하며, 번역에 대한 포괄적인 원어민 검수는 아직 진행되지 않았습니다.

## 라이선스

`0.1.9-test.2`부터 자체 코드, SDK, 문서, 설정과 리소스는 **AGPL-3.0-only**를 적용합니다. test.1 이전 릴리스는 Apache-2.0을, 타사 콘텐츠는 각각의 라이선스를 유지합니다. 배포 패키지에는 라이선스, 저작권 고지와 해당 소스 안내가 포함됩니다.

[AGPL-3.0-only](../../LICENSE) · [NOTICE](../../NOTICE) · [Third-party licenses](../../packaging/THIRD_PARTY_NOTICES.txt)
