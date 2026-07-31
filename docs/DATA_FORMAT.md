# 에디터 데이터 포맷

이 문서는 `besfa_data` crate를 구현하기 전에 확정할 초기 `.besfa` 데이터 포맷의 초안이다.

## 저장 위치

Besfa로 생성한 게임 프로젝트는 에디터 데이터를 프로젝트 루트의 `besfa/` 디렉터리에 저장한다.

```text
my-game/
├─ Cargo.toml
├─ src/
├─ assets/
└─ besfa/
   ├─ project.besfa
   ├─ scenes/
   │  └─ main.besfa
   ├─ terrain/
   └─ animation/
```

`besfa/`는 디렉터리 이름이고, `project.besfa`와 `scenes/*.besfa`는 데이터 파일이다.

## 초기 포맷

- 텍스트 형식: RON
- 파일 확장자: `.besfa`
- 모든 파일은 최상위에 `schema_version`을 가진다.
- 파일은 사람이 직접 편집할 수 있다.
- 파일을 직접 편집한 뒤에는 `besfa validate`로 유효성을 확인한다.

예시:

```ron
(
    schema_version: 1,
    name: "My First Game",
)
```

## 버전 관리

포맷을 호환되지 않게 변경할 때는 `schema_version`을 올린다. `besfa_data`는 현재 버전과 이전 지원 버전의 파일을 읽고, `besfa migrate`가 최신 포맷으로 변환한다.

## 구현 상태

`besfa_data` crate와 실제 `.besfa` 파서는 아직 구현되지 않았다. 이 문서는 구현 전에 검토·확정해야 한다.
