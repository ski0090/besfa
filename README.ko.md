# Besfa

[English](README.md) | **한국어**

Bevy 게임 엔진을 위한 데스크톱 에디터입니다. 에디터 UI는 Flutter, 프로젝트 관리는 `besfa` CLI(Rust)가 담당합니다. 자세한 내용은 [docs/PROJECT_OVERVIEW.md](docs/PROJECT_OVERVIEW.md)를 참고하세요.

## 요구 사항

- Windows
- [Rust](https://rustup.rs/) (`cargo`가 PATH에 있어야 합니다)
- [Flutter](https://docs.flutter.dev/get-started/install/windows/desktop) (Windows 데스크톱 빌드 설정 포함)

## 설치

### 1. CLI 설치

에디터는 PATH에서 `besfa` 실행 파일을 찾습니다.

```bash
cargo install --path crates/besfa_cli
```

`~/.cargo/bin/besfa.exe`가 설치됩니다. 확인:

```bash
besfa --version
```

CLI 코드를 수정했다면 같은 명령을 다시 실행해 갱신하세요.

### 2. 에디터 실행

```bash
cd apps/editor_ui
flutter run -d windows
```

CLI를 설치한 뒤에는 에디터를 다시 시작해야 PATH 변경이 반영됩니다.

## 프로젝트 만들기

- 에디터: Project Hub에서 **New project** → 새 폴더 경로 입력 → **Create**
- CLI: `besfa new C:\Projects\my_game`

대상 폴더는 아직 없어야 하며, 폴더 이름이 프로젝트 이름이 됩니다.

## 테스트

```bash
cargo test
cd apps/editor_ui && flutter test
```
