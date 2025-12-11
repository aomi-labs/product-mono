# AOMI Current State

> Temporary sprint snapshot for agents. Concise but detailed enough to avoid digging through history.

---

## Current Sprint Goal

**Eval Test Framework & DeFi Integration Testing** — Build comprehensive evaluation test suite for onchain agent interactions, including Balancer swaps, ERC20 operations, and Forge script generation.

---

## Branch Status

Current branch: `eval-demo` (base: `main`)

**Recent Commits** (last 10):
```
ea95f391 Merge remote-tracking branch 'origin/main' into eval-demo
3b97d46a Update eval script and result
903508b4 Add balancer tests; add erc20 eval test impl
d768e027 Add test-only tokio runtime handling in scheduler
933542dd Merge pull request #95 from aomi-labs/mono-be-foundry-2
cb40a208 ignored another unit test
99370a2d ignored test that requires baml server
9ba7fe2b restored package-lock.json
9d09b1ee allow some clippy warnings for generated code
742447b3 cleaned up clippy and fmt errors
```

---

## Recently Completed Work

### Eval Test Framework
| Change | Description |
|--------|-------------|
| **Eval crate structure** | Created `aomi/crates/eval/` with modular test harness, assertions, and state management |
| **Test harness** | `Harness` struct manages test cases, runs suites, verifies expectations and assertions |
| **EvalState** | Per-test session state with round tracking, message history, and agent interaction |
| **Assertion system** | Balance checks, balance changes, ERC20 token assertions with tolerance handling |
| **Evaluation app** | Headless evaluation agent for automated test verification |

### Test Suite Implementation
| Change | Description |
|--------|-------------|
| **Basic tests (4)** | ETH balance check, ETH transfer, ETH→USDC swap, Balancer swap |
| **DeFi tests (10)** | ERC20 approvals, transfers, Uniswap V2 liquidity, Lido staking, Aave supply/borrow |
| **Scripter tests (2)** | Forge script generation for ETH transfers and ERC20 approvals |
| **Test runner script** | `scripts/run-eval-tests.sh` bootstraps Anvil, runs tests, generates markdown reports |

### Infrastructure Improvements
| Change | Description |
|--------|-------------|
| **Test-only runtime** | Scheduler creates own tokio runtime when `cfg!(test)` or `feature = "eval-test"` (scheduler.rs:107) |
| **Anvil integration** | Eval tests run against local Anvil fork of Ethereum mainnet |
| **Account prefunding** | Automatic USDC prefunding via whale impersonation for test accounts |
| **Result reporting** | Markdown output with test summaries, evaluation verdicts, and full logs |

### Code Quality
| Change | Description |
|--------|-------------|
| **Clippy fixes** | Fixed warnings in generated code and non-generated code |
| **Test organization** | Tests marked with `#[ignore]` and run via script for controlled execution |
| **Error handling** | Graceful skipping when ANTHROPIC_API_KEY or BAML server unavailable |

---

## Files Modified This Sprint

### Eval Framework Core
| File | Key Changes |
|------|-------------|
| `aomi/crates/eval/src/lib.rs` | Core types: `TestResult`, `RoundResult`, `AgentAction`, `ToolCall` |
| `aomi/crates/eval/src/harness.rs` | `Harness` struct, `EvalCase` builder, assertion verification |
| `aomi/crates/eval/src/eval_state.rs` | `EvalState` for per-test session management, round tracking |
| `aomi/crates/eval/src/eval_app.rs` | `EvaluationApp` for headless evaluation agent |
| `aomi/crates/eval/src/assertions.rs` | Balance assertions, ERC20 token handling, onchain verification |

### Test Implementations
| File | Key Changes |
|------|-------------|
| `aomi/crates/eval/src/test_entry.rs` | **NEW**: 14 test cases covering basic ops, DeFi, and multi-step flows |
| `aomi/crates/eval/src/test_scripter.rs` | **NEW**: 2 Forge script generation tests |

### Infrastructure
| File | Key Changes |
|------|-------------|
| `aomi/crates/tools/src/scheduler.rs` | Test-only tokio runtime creation (line 107: `cfg!(test) || cfg!(feature = "eval-test")`) |
| `scripts/run-eval-tests.sh` | **NEW**: Complete test runner with Anvil bootstrap, result parsing, markdown output |
| `aomi/crates/eval/Cargo.toml` | Eval crate dependencies, `eval-test` feature flag |

---

## Pending Tasks

### Immediate Priority

1. **Test stability improvements**
   - Some tests may be flaky (e.g., `test_swap_eth_for_usdc_on_balancer` failed in recent run)
   - Investigate timeout handling and retry logic
   - Improve error messages for assertion failures

2. **Test coverage expansion**
   - Add more complex DeFi scenarios (multi-hop swaps, yield farming)
   - Add edge case tests (insufficient balance, failed transactions)
   - Add negative test cases (invalid addresses, wrong networks)

### Short-Term

3. **CI/CD integration**
   - Add eval tests to CI pipeline
   - Configure test environment (Anvil, BAML server)
   - Set up test result artifact storage

4. **Documentation**
   - Document test writing patterns
   - Add examples for custom assertions
   - Create guide for running eval tests locally

5. **Performance optimization**
   - Parallel test execution where possible
   - Reduce Anvil startup time
   - Optimize assertion verification

---

## Known Issues

| Issue | Status | Notes |
|-------|--------|-------|
| Balancer swap test failing | Open | Test `test_swap_eth_for_usdc_on_balancer` failed in recent run; needs investigation |
| BAML server dependency | Working | Scripter tests require BAML server at `localhost:2024` |
| Anvil fork stability | Working | Tests use mainnet fork; may need block number pinning for reproducibility |
| Test execution time | Working | Some tests take 30-90s due to LLM calls; consider caching or mocking |

---

## Multi-Step Flow State

Current Position: Core Framework Complete, Test Suite Expanding

| Step | Description | Status |
|------|-------------|--------|
| 1 | Create eval crate structure | ✓ Done |
| 2 | Implement test harness and state management | ✓ Done |
| 3 | Add assertion system for balance/token checks | ✓ Done |
| 4 | Implement basic test cases | ✓ Done |
| 5 | Add DeFi test cases (Uniswap, Aave, Lido) | ✓ Done |
| 6 | Add Balancer swap test | ✓ Done |
| 7 | Add Forge scripter tests | ✓ Done |
| 8 | Create test runner script | ✓ Done |
| 9 | Fix test-only runtime handling | ✓ Done |
| 10 | Improve test stability | ⏳ In Progress |
| 11 | Expand test coverage | ⏳ Pending |
| 12 | CI/CD integration | ⏳ Pending |

---

## Test Results

### Test Suite Overview
**Location**: `aomi/crates/eval/src/test_entry.rs`

**Run command**:
```bash
./scripts/run-eval-tests.sh [test_filter]
```

**Test Categories**:
- **Basic (4)**: Balance checks, transfers, simple swaps
- **DeFi (10)**: ERC20 ops, Uniswap V2, Lido, Aave
- **Scripter (2)**: Forge script generation

**Latest Results** (from `output/eval-results.md`):
- ❌ `test_swap_eth_for_usdc_on_balancer` failed
- Other tests status: See latest run output

**Prerequisites**:
- Anvil running (auto-started by script)
- `ANTHROPIC_API_KEY` in `.env.dev`
- `ALCHEMY_API_KEY` in `.env.dev` (for mainnet fork)
- BAML server at `localhost:2024` (for scripter tests)

---

## Notes for Next Agent

### Critical Context

1. **Eval Test Framework**
   - Tests run against local Anvil fork of Ethereum mainnet
   - Uses deterministic test accounts (Alice, Bob) from `EVAL_ACCOUNTS`
   - Tests are marked `#[ignore]` and run via `scripts/run-eval-tests.sh`
   - Requires `ANTHROPIC_API_KEY` for LLM agent interactions

2. **Test-Only Runtime**
   - Scheduler creates own tokio runtime when `cfg!(test)` or `feature = "eval-test"`
   - This prevents runtime conflicts in test contexts
   - See `scheduler.rs:107` for the condition

3. **Assertion System**
   - Balance assertions support tolerance for slippage/gas
   - ERC20 tokens require correct decimals (USDC=6, stETH=18, etc.)
   - Assertions verify onchain state after agent actions

4. **Test Runner Script**
   - Bootstraps Anvil with mainnet fork
   - Prefunds Alice with USDC via whale impersonation
   - Parses cargo test output and generates markdown reports
   - Output saved to `output/eval-results.md`

### Quick Start Commands
```bash
# Run all eval tests
./scripts/run-eval-tests.sh

# Run specific test
./scripts/run-eval-tests.sh test_swap_eth_for_usdc_on_balancer

# Run scripter tests (requires BAML server)
./scripts/run-eval-tests.sh test_simple_eth_transfer_script

# Check test results
cat output/eval-results.md
```

### Code References

**Key files and line numbers**:
- Test-only runtime: `scheduler.rs:107` (`cfg!(test) || cfg!(feature = "eval-test")`)
- Test harness: `harness.rs:318` (`default_with_cases`)
- Eval state: `eval_state.rs:112` (`EvalState::new`)
- Balancer test: `test_entry.rs:186` (`test_swap_eth_for_usdc_on_balancer`)
- Test runner: `scripts/run-eval-tests.sh:108` (cargo test command)
