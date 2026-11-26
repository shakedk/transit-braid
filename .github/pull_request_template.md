- **Architectural Why:** <!-- The system design intent behind this change. -->
- **Blast Radius:** <!-- "Trunk" (core architecture, types, security) or "Leaf" (isolated helper), and why. -->
- **Data Flow:** <!-- How data or state flows through these changes. -->

## 👥 Human Maintainer Review Checklist

### 1. Triage by Blast Radius

- [ ] **Classify the PR:** Is this "Trunk" (core architecture, types, security) or "Leaf" (isolated helper)?
- [ ] **Line-by-Line Audit:** If "Trunk", I have manually read every single line of code. If "Leaf", I skipped the code syntax review.

### 2. Contract & Dependency Check

- [ ] **Public API Verification:** Confirmed that public interfaces and export signatures remain unbroken.
- [ ] **Zero Rogue Packages:** Verified no unauthorized third-party libraries were smuggled into the code.

### 3. Automated Guardrail Verification

- [ ] **Anti-Lazy Test Check:** Confirmed that tests actually assert logic and do not just use empty placeholders or blind mocks.
- [ ] **Error & Null Inputs:** Verified that tests explicitly throw malformed, missing, and null payloads at the new code.
- [ ] **Security Scans:** CI pipeline reports 0 new vulnerabilities (SAST) or exposed secrets.
