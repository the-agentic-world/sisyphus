# Megara

[![CI](https://github.com/the-agentic-world/megara/actions/workflows/ci.yml/badge.svg)](https://github.com/the-agentic-world/megara/actions/workflows/ci.yml)
[![Release](https://github.com/the-agentic-world/megara/actions/workflows/release.yml/badge.svg)](https://github.com/the-agentic-world/megara/actions/workflows/release.yml)

## 프로젝트 소개

Megara라는 이름은 메가라 학파에서 가져왔습니다. 메가라 학파는 소크라테스의 제자 에우클레이데스가 세운 학파로, 논리와 변증, 엄밀한 논박을 중시했습니다. 여러 역할의 에이전트가 요구사항, 계획, 실행, 검증을 그냥 이어 붙이는 것이 아니라 서로 따지고 검토하며 더 단단한 결론으로 수렴한다는 이미지와 맞닿아 있습니다.

[가재코드(GJC)](https://github.com/Yeachan-Heo/gajae-code)의 하네스 실험에서 출발했지만, Megara의 현재 제품 경계는 결정론적 설치기와 Planning Core입니다. 상태·전이·승인·증거는 하나의 typed planning store가 소유하고, Codex와 Pi는 요청·표시를 담당하는 어댑터로 남습니다.

저장소의 `harness/` 디렉터리가 내장 하네스의 source of truth입니다. `megara install`은 이 파일들을 선택한 범위의 `.agents/` 또는 `~/.megara`에 설치하고, Codex 또는 Pi Coding Agent가 읽을 수 있는 형태로 투영합니다.

핵심 Planning Core:

- typed work item과 명시적 질문·답변 전이를 저장합니다.
- spec·plan·evidence를 현재 revision과 함께 생성하고 승인합니다.
- 프로젝트별 planning DB를 `.megara/planning`에 보존합니다.
- Codex MCP와 Pi 확장은 같은 core service 경계를 사용합니다.

포함된 역할 에이전트:

| 역할 | Codex 모델 | 추론 수준 |
| --- | --- | --- |
| `executor` | `gpt-5.6-terra` | `high` |
| `planner` | `gpt-5.6-terra` | `high` |
| `architect` | `gpt-5.6-sol` | `xhigh` |
| `critic` | `gpt-5.6-sol` | `high` |
| `researcher` | `gpt-5.6-terra` | `medium` |
| `contrarian` | `gpt-5.6-sol` | `high` |
| `simplifier` | `gpt-5.6-luna` | `high` |

GPT-5.6 역할 프로필에는 ChatGPT 데스크톱 앱 `26.707.30751` 이상 또는 Codex CLI `0.144.0` 이상이 필요합니다. Megara command adapter는 사용 중인 실행 환경의 버전을 확인하고, 최소 버전보다 낮으면 업데이트 안내를 표시합니다.

내장 기본 활성 스킬:

- `caveman`: [juliusbrussee/caveman](https://github.com/juliusbrussee/caveman)을 Megara에 내장한 짧은 응답 압축 스킬입니다. 별도 설치 없이 하네스와 함께 설치되고, 새 세션과 재개 세션에서 기본 활성화됩니다.

내장 온디맨드 스킬:

- `insane-search`: [fivetaku/insane-search](https://github.com/fivetaku/insane-search)를 `$insane-search`로 호출할 수 있게 노출한 스킬입니다. 실제 실행 엔진은 아래 온디맨드 도구를 사용하며, 기본 활성 스킬로 등록하지 않습니다.

내장 온디맨드 도구:

- `insane-search`: [fivetaku/insane-search](https://github.com/fivetaku/insane-search)를 Megara 도구로 내장한 공개 웹 접근 보조 도구입니다. 일반 search/fetch가 실패하거나 차단/JS-heavy 페이지를 다뤄야 할 때만 사용하며, 기본 활성 스킬로 등록하지 않습니다.

## 설치안내

최신 릴리스를 설치합니다.

```bash
curl -fsSL https://github.com/the-agentic-world/megara/releases/latest/download/install.sh | sh
```

특정 버전이나 설치 위치를 지정할 수 있습니다.

```bash
curl -fsSL https://github.com/the-agentic-world/megara/releases/latest/download/install.sh | MEGARA_VERSION=v<version> MEGARA_INSTALL_DIR="$HOME/.local/bin" sh
```

설치 스크립트는 macOS arm64와 Linux x86_64를 지원하며 기본 설치 위치는 `$HOME/.local/bin`입니다. 기본 경로가 쓸 수 없으면 다른 사용자 쓰기 가능 경로로 설치를 시도합니다. 설치 후 `megara` 명령을 바로 사용하려면 설치 경로가 `PATH`에 포함되어 있어야 합니다. 이전 기본 위치에 남은 Megara 바이너리는 `sudo` 없이 제거를 시도하며, 권한상 제거할 수 없으면 직접 제거 또는 `PATH` 우선순위 조정을 안내합니다.

Homebrew로도 설치할 수 있습니다.

```bash
brew install the-agentic-world/tap/megara
```

소스에서 직접 빌드하려면 Rust toolchain이 필요합니다.

```bash
cargo build --release
./target/release/megara --version
```

## 사용법

설치 wizard를 실행합니다. 첫 문항에서 사용자-facing 응답 locale을 선택합니다.

```bash
megara install
```

현재 프로젝트에 Codex용 하네스를 설치합니다.

```bash
megara install --scope project --target codex --trust-project
```

현재 프로젝트에 Pi Coding Agent용 하네스를 설치하고, 생성된 역할 에이전트 실행을 신뢰합니다.

```bash
megara install --scope project --target pi --trust-project
```

Pi는 `@earendil-works/pi-coding-agent >=0.80.10, <0.81.0`을 요구합니다. `--trust-project` 없이 설치하면 파일은 생성되지만 역할 에이전트 실행은 차단됩니다. 대화형 설치에서는 같은 신뢰 결정을 묻습니다.

Codex의 프로젝트 `.codex/config.toml`은 프로젝트를 신뢰할 때만 활성화됩니다. 내용을 검토한 뒤 `--trust-project`를 사용하세요. 이 옵션 없이 설치하면 파일은 생성되지만 Codex 설정은 비활성 상태로 남고 `doctor`가 이를 알립니다. 자세한 신뢰 규칙은 [Codex config reference](https://learn.chatgpt.com/docs/config-file/config-reference)를 참고하세요.

Megara는 설치·sync·update 때 `codex features list`로 `default_mode_request_user_input`을 확인합니다. 런타임이 이 기능을 광고하면 해당 설정을 활성화해 자연스러운 선택형 질문을 사용하고, 구버전·미설치·미지원 런타임에서는 기존 Markdown 질문 흐름을 유지합니다. Megara가 추가한 설정만 이후 sync/update에서 정리하며, 사용자가 명시한 `false` 설정은 덮어쓰지 않습니다.

역할별 모델 정책을 대화형으로 설정합니다.

```bash
megara agents configure
```

비대화형 환경에서는 scope, target, role, model, 추론 수준을 명시합니다. project 설정은 global 기본값을 덮어씁니다.

```bash
megara agents configure \
  --scope project --target codex --role executor \
  --model gpt-5.6-sol --reasoning-effort xhigh

megara agents show --scope project --target codex
megara agents reset --scope project --target codex --role executor
```

에이전트에게는 `$agent-models` 스킬을 요청할 수 있습니다. 스킬은 현재 정책과 변경안을 제시하지만, 사용자의 명시적 승인 전에는 설정을 변경하지 않습니다.
Megara가 관리하지 않는 `megara.toml`은 보호하며, 명시적으로 교체하려면 `megara agents configure --force` 또는 `megara agents reset --force`를 사용합니다.

locale을 명시해 비대화형 설치도 할 수 있습니다.

```bash
megara install --scope project --target codex --locale ko-KR
```

전역 범위에 설치합니다.

```bash
megara install --scope global --target codex
```

설치 상태와 drift를 확인합니다.

```bash
megara doctor --scope project --target codex
```

Megara 바이너리와 설치된 하네스를 최신 릴리스 기준으로 업데이트합니다.

```bash
megara update
```

`megara update`는 바이너리 확인 후 설치된 하네스도 다시 투영합니다. 프로젝트 범위 설치에서는 이전 버전이 만든 Megara-managed `.codex/skills/*/SKILL.md` 파일도 함께 제거해 Codex App 스킬 중복 표시를 정리합니다.

특정 범위만 업데이트할 수 있습니다.

```bash
megara update --scope project
megara update --scope global
```

설치된 `.agents/` 또는 `~/.megara` source of truth에서 런타임 파일을 다시 투영합니다. 인자 없이 실행하면 현재 scope의 managed runtime만 탐색해 동기화합니다.

```bash
megara sync

# 특정 runtime만 동기화
megara sync --scope project --target codex
```

설치된 runtime projection을 제거합니다. Megara 관리 파일만 지우며, planning과 tool runtime data는 보존합니다. Pi와 Codex를 함께 설치했다면 한 runtime을 제거해도 공유 SSOT는 남습니다.

```bash
megara uninstall --scope project --target codex

# 변경 없이 제거 대상 확인
megara uninstall --scope project --target codex --dry-run
```

지원 대상과 템플릿을 확인합니다.

```bash
megara targets list
megara templates list
```

사용자 요청으로 남기는 지식 문서는 OKF bundle로 정리할 수 있습니다.

```bash
megara docs init
megara docs check
```

기본 root는 `docs/`입니다. 다른 위치를 쓰려면 `--root`를 지정합니다.

```bash
megara docs init --root knowledge
megara docs check --root knowledge
```

`megara docs init`은 `index.md`와 `log.md` scaffold만 생성합니다. 사용자 문서이므로 `MEGARA:MANAGED` marker를 넣지 않습니다. `megara docs check`는 OKF v0.1 최소 conformance를 확인하며, runtime artifact인 `.megara/**`, skill 파일인 `.agents/skills/**`, Megara 저장소의 제품 하네스 소스인 `harness/**`는 검사 대상에서 제외합니다.

설치 범위는 두 가지입니다.

- `project`: 현재 프로젝트의 `.agents/`에 SSOT를 쓰고, Codex는 `.codex/`, Pi는 `.pi/`로 파일을 투영합니다.
- `global`: `~/.megara`에 SSOT를 쓰고, Codex는 `~/.codex/`, Pi는 `~/.pi/agent/`으로 파일을 투영합니다.

Megara는 기본적으로 기존 사용자 파일을 보호합니다. 목적지가 Megara 관리 파일이 아니면 충돌을 보고하고 그대로 둡니다. Megara가 파일 소유권을 가져가야 할 때만 `--force`를 사용하세요.

Planning migration과 purge는 먼저 논리 상태와 event 경계를 확정한 뒤 filesystem cleanup을 수행합니다. migration은 `--dry-run`으로 범위를 확인하고, 중단 시 `--resume` 또는 검증된 `--rollback`을 사용합니다. purge 뒤 `doctor --repair`는 event를 수정하지 않고 replay cache, managed Markdown projection, artifact·backup cleanup residue만 재시도합니다. SSD wear-leveling이나 외부 backup까지 포함하는 forensic erase는 보장하지 않습니다. 자세한 운영 절차는 [Planning Core v1 운영·마이그레이션·릴리스 절차](docs/plan/planning-core-v1-release-operations.md)를 참고하세요.

Megara는 일반 CLI 명령 사용 시 하루에 한 번 최신 릴리스를 확인합니다. 새 버전이 있으면 stderr에 `megara update` 안내만 표시하고 자동으로 변경하지 않습니다. Planning RPC와 command adapter 실행 중에는 업데이트 체크를 하지 않으며, 자동 체크를 끄려면 `MEGARA_NO_UPDATE_CHECK=1`을 설정하세요.

### 프롬프트로 Planning Core 사용하기

프로젝트 범위 설치 후에는 해당 프로젝트를 새 Codex 세션으로 열고 생성된 `AGENTS.md`와 skills를 사용합니다. 이미 열려 있던 세션은 새 세션을 열거나 현재 세션에서 해당 파일을 다시 읽어야 변경된 지침을 반영합니다.

프로젝트 범위 Codex 설치에서는 Megara 스킬을 `.agents/skills`에만 둡니다. Codex App이 이 디렉터리를 직접 읽기 때문에 같은 스킬을 `.codex/skills`에도 복사하면 스킬 목록이 중복됩니다. 이전 버전이 만든 Megara-managed `.codex/skills/*/SKILL.md` 파일은 `megara sync`가 제거합니다.

Megara에는 `caveman`이 내장되어 있어 기본 응답이 짧게 압축됩니다. 일반 문체가 필요하면 다음처럼 요청합니다.

```text
normal mode
```

다시 켜거나 강도를 바꿀 때는 다음처럼 요청합니다.

```text
/caveman lite
/caveman full
/caveman ultra
```

일반 검색이나 fetch가 막히는 공개 페이지를 다룰 때는 내장 도구를 요청합니다.

```text
insane-search 도구로 이 URL을 공개 접근 가능한 경로부터 확인해줘: https://example.com/
```

프로젝트 범위 설치에서는 스킬 래퍼가 `.agents/skills/insane-search`에 있고, 도구 파일은 `.agents/tools/insane-search`, 실행 wrapper는 `.agents/bin/insane-search`입니다. 첫 실행 시 wrapper가 `.megara/state/tools/insane-search/venv`에 필요한 Python dependency를 자동 bootstrap합니다. 이 스킬은 상시 활성 스킬이 아니므로 단순 검색에는 개입하지 않습니다.

Planning Core를 사용하는 일반적인 흐름은 다음과 같습니다.

1. `megara planning start`로 현재 요청과 project context를 제출합니다.
2. 질문이 반환되면 `megara planning answer`로 사용자의 답변을 제출합니다.
3. 반환된 typed proposal을 검토하고 `megara planning spec approve` 또는 `megara planning plan approve`로 명시적으로 승인합니다.
4. `megara planning evidence refresh`와 `megara planning audit apply`로 변경·검증 증거를 갱신합니다.
5. 완료 후에는 `megara planning purge`로 보존 정책에 맞게 세션을 정리합니다.

## 현재 제약사항

- 지원 런타임은 Codex와 Pi Coding Agent입니다. Pi 역할 에이전트는 프로젝트 설치 시 명시적 신뢰가 필요합니다.
- 릴리스 설치 스크립트와 공식 binary는 macOS arm64와 Linux x86_64를 지원합니다. Windows, Linux arm64, macOS Intel은 현재 release artifact 대상이 아니며 소스 빌드가 필요합니다.
- Codex App은 프로젝트의 생성된 `AGENTS.md`와 configured skills를 읽습니다. 프로젝트 범위 설치 후에는 저장된 프로젝트 또는 정확한 설치 디렉터리로 새 세션을 열어야 합니다.
- 프로젝트 범위 Codex 설치는 스킬 중복 표시를 피하기 위해 `.agents/skills`를 사용하고 `.codex/skills`로 스킬을 복사하지 않습니다.
- 프로젝트 없는 Codex App 세션은 `name-2` 같은 sibling 디렉터리를 만들 수 있습니다. 이 경우 설치한 `.agents/`와 `.codex/`가 없는 위치에서 세션이 시작될 수 있습니다.
- Planning Core 상태는 `.megara/planning`의 store와 지원되는 `megara planning` 명령이 관리합니다. planning DB를 직접 편집해 전이를 우회하지 마세요.
- 기본 내장 하네스 locale은 `ko-KR`입니다. 파일 경로, 명령어, config key 같은 기술 literal은 그대로 유지됩니다.

## GJC 저장소

Megara는 GJC 하네스가 보여준 작업 방식에서 출발했습니다. 원본 아이디어와 더 큰 실험 맥락이 궁금하다면 [Yeachan-Heo/gajae-code](https://github.com/Yeachan-Heo/gajae-code) 저장소도 함께 살펴보세요.
