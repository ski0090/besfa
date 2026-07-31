# CLI 계약

이 문서는 Flutter UI와 `besfa_cli`가 프로젝트 생성 시 사용할 초기 기계 판독 계약의 초안이다. 실제 구현 전에 검토하고 확정한다.

## 명령

```text
besfa new <DIRECTORY> --template <TEMPLATE> --output json
```

`DIRECTORY`는 생성될 게임 프로젝트의 최종 루트 디렉터리다.

## JSON 출력

`--output json` 모드에서는 표준 출력에 결과 JSON 객체 하나만 출력한다. 표준 오류는 사람이 읽는 진단 정보에만 사용한다.

성공 예시:

```json
{
  "status": "success",
  "project_path": "D:\\Projects\\my_first_game"
}
```

실패 예시:

```json
{
  "status": "error",
  "code": "destination_exists",
  "message": "The destination directory already exists."
}
```

## 종료 코드

| 코드 | 의미 |
| --- | --- |
| `0` | 성공 |
| `2` | 잘못된 CLI 인자 (Clap) |
| `10` | 대상 디렉터리가 이미 존재함 |
| `11` | 유효하지 않은 프로젝트 이름 또는 요청 |
| `12` | 존재하지 않는 템플릿 |
| `20` | 파일 시스템 오류 |
| `30` | 예상하지 못한 내부 오류 |

## 대상 디렉터리 정책

- 대상 디렉터리가 존재하면, 비어 있어도 생성에 실패한다.
- 생성기는 대상과 같은 부모 디렉터리 아래의 임시 디렉터리에서 작업한다.
- 모든 파일 작성과 검증이 성공한 뒤에만 최종 디렉터리로 이동한다.
- 실패한 생성 작업은 임시 디렉터리를 정리한다.

## 구현 상태

현재 `besfa new`는 Clap 인자 파싱만 구현되어 있다. 이 문서의 JSON 출력, 종료 코드, 파일 생성 정책은 아직 구현되지 않았다.
