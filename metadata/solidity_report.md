
[<img width="200" alt="get in touch with Consensys Diligence" src="https://user-images.githubusercontent.com/2865694/56826101-91dcf380-685b-11e9-937c-af49c2510aa0.png">](https://consensys.io/diligence)<br/>
<sup>
[[  🌐  ](https://consensys.io/diligence)  [  📩  ](mailto:diligence@consensys.net)  [  🔥  ](https://consensys.io/diligence/tools/)]
</sup><br/><br/>



# Solidity Metrics for 'CLI'

## Table of contents

- [Scope](#t-scope)
    - [Source Units in Scope](#t-source-Units-in-Scope)
        - [Deployable Logic Contracts](#t-deployable-contracts)
    - [Out of Scope](#t-out-of-scope)
        - [Excluded Source Units](#t-out-of-scope-excluded-source-units)
        - [Duplicate Source Units](#t-out-of-scope-duplicate-source-units)
        - [Doppelganger Contracts](#t-out-of-scope-doppelganger-contracts)
- [Report Overview](#t-report)
    - [Risk Summary](#t-risk)
    - [Source Lines](#t-source-lines)
    - [Inline Documentation](#t-inline-documentation)
    - [Components](#t-components)
    - [Exposed Functions](#t-exposed-functions)
    - [StateVariables](#t-statevariables)
    - [Capabilities](#t-capabilities)
    - [Dependencies](#t-package-imports)
    - [Totals](#t-totals)

## <span id=t-scope>Scope</span>

This section lists files that are in scope for the metrics report. 

- **Project:** `'CLI'`
- **Included Files:** 
    - ``
- **Excluded Paths:** 
    - ``
- **File Limit:** `undefined`
    - **Exclude File list Limit:** `undefined`

- **Workspace Repository:** `unknown` (`undefined`@`undefined`)

### <span id=t-source-Units-in-Scope>Source Units in Scope</span>

Source Units Analyzed: **`3`**<br>
Source Units in Scope: **`3`** (**100%**)

| Type | File   | Logic Contracts | Interfaces | Lines | nLines | nSLOC | Comment Lines | Complex. Score | Capabilities |
| ---- | ------ | --------------- | ---------- | ----- | ------ | ----- | ------------- | -------------- | ------------ | 
| 🔍 | contracts/src/ISecondOpinionOracle.sol | **** | 1 | 16 | 6 | 3 | 2 | 3 | **** |
| 📝 | contracts/src/PausableUntil.sol | 1 | **** | 102 | 102 | 72 | 14 | 32 | **** |
| 📝 | contracts/src/Sp1LidoAccountingReportContract.sol | 1 | **** | 391 | 372 | 245 | 69 | 119 | **<abbr title='TryCatch Blocks'>♻️</abbr><abbr title='Unchecked Blocks'>Σ</abbr>** |
| 📝🔍 | **Totals** | **2** | **1** | **509**  | **480** | **320** | **85** | **154** | **<abbr title='TryCatch Blocks'>♻️</abbr><abbr title='Unchecked Blocks'>Σ</abbr>** |

<sub>
Legend: <a onclick="toggleVisibility('table-legend', this)">[➕]</a>
<div id="table-legend" style="display:none">

<ul>
<li> <b>Lines</b>: total lines of the source unit </li>
<li> <b>nLines</b>: normalized lines of the source unit (e.g. normalizes functions spanning multiple lines) </li>
<li> <b>nSLOC</b>: normalized source lines of code (only source-code lines; no comments, no blank lines) </li>
<li> <b>Comment Lines</b>: lines containing single or block comments </li>
<li> <b>Complexity Score</b>: a custom complexity score derived from code statements that are known to introduce code complexity (branches, loops, calls, external interfaces, ...) </li>
</ul>

</div>
</sub>


##### <span id=t-deployable-contracts>Deployable Logic Contracts</span>
Total: 1
* 📝 `Sp1LidoAccountingReportContract`



#### <span id=t-out-of-scope>Out of Scope</span>

##### <span id=t-out-of-scope-excluded-source-units>Excluded Source Units</span>

Source Units Excluded: **`0`**

<a onclick="toggleVisibility('excluded-files', this)">[➕]</a>
<div id="excluded-files" style="display:none">
| File   |
| ------ |
| None |

</div>


##### <span id=t-out-of-scope-duplicate-source-units>Duplicate Source Units</span>

Duplicate Source Units Excluded: **`0`** 

<a onclick="toggleVisibility('duplicate-files', this)">[➕]</a>
<div id="duplicate-files" style="display:none">
| File   |
| ------ |
| None |

</div>

##### <span id=t-out-of-scope-doppelganger-contracts>Doppelganger Contracts</span>

Doppelganger Contracts: **`0`** 

<a onclick="toggleVisibility('doppelganger-contracts', this)">[➕]</a>
<div id="doppelganger-contracts" style="display:none">
| File   | Contract | Doppelganger | 
| ------ | -------- | ------------ |


</div>


## <span id=t-report>Report</span>

### Overview

The analysis finished with **`0`** errors and **`0`** duplicate files.





#### <span id=t-risk>Risk</span>

<div class="wrapper" style="max-width: 512px; margin: auto">
			<canvas id="chart-risk-summary"></canvas>
</div>

#### <span id=t-source-lines>Source Lines (sloc vs. nsloc)</span>

<div class="wrapper" style="max-width: 512px; margin: auto">
    <canvas id="chart-nsloc-total"></canvas>
</div>

#### <span id=t-inline-documentation>Inline Documentation</span>

- **Comment-to-Source Ratio:** On average there are`4.11` code lines per comment (lower=better).
- **ToDo's:** `0` 

#### <span id=t-components>Components</span>

| 📝Contracts   | 📚Libraries | 🔍Interfaces | 🎨Abstract |
| ------------- | ----------- | ------------ | ---------- |
| 2 | 0  | 1  | 0 |

#### <span id=t-exposed-functions>Exposed Functions</span>

This section lists functions that are explicitly declared public or payable. Please note that getter methods for public stateVars are not included.  

| 🌐Public   | 💰Payable |
| ---------- | --------- |
| 12 | 0  | 

| External   | Internal | Private | Pure | View |
| ---------- | -------- | ------- | ---- | ---- |
| 7 | 27  | 1 | 0 | 17 |

#### <span id=t-statevariables>StateVariables</span>

| Total      | 🌐Public  |
| ---------- | --------- |
| 14  | 10 |

#### <span id=t-capabilities>Capabilities</span>

| Solidity Versions observed | 🧪 Experimental Features | 💰 Can Receive Funds | 🖥 Uses Assembly | 💣 Has Destroyable Contracts | 
| -------------------------- | ------------------------ | -------------------- | ---------------- | ---------------------------- |
| `0.8.27` |  | **** | **** | **** | 

| 📤 Transfers ETH | ⚡ Low-Level Calls | 👥 DelegateCall | 🧮 Uses Hash Functions | 🔖 ECRecover | 🌀 New/Create/Create2 |
| ---------------- | ----------------- | --------------- | ---------------------- | ------------ | --------------------- |
| **** | **** | **** | **** | **** | **** | 

| ♻️ TryCatch | Σ Unchecked |
| ---------- | ----------- |
| `yes` | `yes` |

#### <span id=t-package-imports>Dependencies / External Imports</span>

| Dependency / Import Path | Count  | 
| ------------------------ | ------ |
| @openzeppelin/contracts/access/Ownable.sol | 1 |
| @sp1-contracts/ISP1Verifier.sol | 1 |

#### <span id=t-totals>Totals</span>

##### Summary

<div class="wrapper" style="max-width: 90%; margin: auto">
    <canvas id="chart-num-bar"></canvas>
</div>

##### AST Node Statistics

###### Function Calls

<div class="wrapper" style="max-width: 90%; margin: auto">
    <canvas id="chart-num-bar-ast-funccalls"></canvas>
</div>

###### Assembly Calls

<div class="wrapper" style="max-width: 90%; margin: auto">
    <canvas id="chart-num-bar-ast-asmcalls"></canvas>
</div>

###### AST Total

<div class="wrapper" style="max-width: 90%; margin: auto">
    <canvas id="chart-num-bar-ast"></canvas>
</div>

##### Inheritance Graph

<a onclick="toggleVisibility('surya-inherit', this)">[➕]</a>
<div id="surya-inherit" style="display:none">
<div class="wrapper" style="max-width: 512px; margin: auto">
    <div id="surya-inheritance" style="text-align: center;"></div> 
</div>
</div>

##### CallGraph

<a onclick="toggleVisibility('surya-call', this)">[➕]</a>
<div id="surya-call" style="display:none">
<div class="wrapper" style="max-width: 512px; margin: auto">
    <div id="surya-callgraph" style="text-align: center;"></div>
</div>
</div>

###### Contract Summary

<a onclick="toggleVisibility('surya-mdreport', this)">[➕]</a>
<div id="surya-mdreport" style="display:none">
 Sūrya's Description Report

 Files Description Table


|  File Name  |  SHA-1 Hash  |
|-------------|--------------|
| contracts/src/ISecondOpinionOracle.sol | aae4a41a136aa0bae4c90f868431f5863f86ddad |
| contracts/src/PausableUntil.sol | f384a4a37d3c1cd3311fce52acb734d3515df644 |
| contracts/src/Sp1LidoAccountingReportContract.sol | ca40f4304330c119c9a8021838a95d90dddacaca |


 Contracts Description Table


|  Contract  |         Type        |       Bases      |                  |                 |
|:----------:|:-------------------:|:----------------:|:----------------:|:---------------:|
|     └      |  **Function Name**  |  **Visibility**  |  **Mutability**  |  **Modifiers**  |
||||||
| **SecondOpinionOracle** | Interface |  |||
| └ | getReport | External ❗️ |   |NO❗️ |
||||||
| **PausableUntil** | Implementation |  |||
| └ | _checkPaused | Internal 🔒 |   | |
| └ | _checkResumed | Internal 🔒 |   | |
| └ | isPaused | Public ❗️ |   |NO❗️ |
| └ | getResumeSinceTimestamp | External ❗️ |   |NO❗️ |
| └ | _resume | Internal 🔒 | 🛑  | |
| └ | _pauseFor | Internal 🔒 | 🛑  | |
| └ | _pauseUntil | Internal 🔒 | 🛑  | |
| └ | _setPausedState | Internal 🔒 | 🛑  | |
||||||
| **Sp1LidoAccountingReportContract** | Implementation | SecondOpinionOracle, Ownable, PausableUntil |||
| └ | <Constructor> | Public ❗️ | 🛑  | Ownable |
| └ | getReport | External ❗️ |   |NO❗️ |
| └ | getLatestLidoValidatorStateSlot | Public ❗️ |   |NO❗️ |
| └ | getLidoValidatorStateHash | Public ❗️ |   |NO❗️ |
| └ | getBeaconBlockHash | Public ❗️ |   |NO❗️ |
| └ | submitReportData | Public ❗️ | 🛑  | whenResumed |
| └ | setPauser | External ❗️ | 🛑  | onlyOwner |
| └ | pauseFor | External ❗️ | 🛑  |NO❗️ |
| └ | pauseUntil | External ❗️ | 🛑  |NO❗️ |
| └ | resume | External ❗️ | 🛑  | onlyOwner |
| └ | _verify_reference_and_bc_slot | Internal 🔒 |   | |
| └ | _verify_public_values | Internal 🔒 |   | |
| └ | _getExpectedWithdrawalCredentials | Internal 🔒 |   | |
| └ | _findBeaconBlockHash | Internal 🔒 |   | |
| └ | _blockExists | Internal 🔒 |   | |
| └ | _getBeaconBlockHashForTimestamp | Internal 🔒 |   | |
| └ | _require_for_refslot | Private 🔐 |   | |
| └ | _slotToTimestamp | Internal 🔒 |   | |
| └ | _recordReport | Internal 🔒 | 🛑  | |
| └ | _recordLidoValidatorStateHash | Internal 🔒 | 🛑  | |


 Legend

|  Symbol  |  Meaning  |
|:--------:|-----------|
|    🛑    | Function can modify state |
|    💵    | Function is payable |
 

</div>
____
<sub>
Thinking about smart contract security? We can provide training, ongoing advice, and smart contract auditing. [Contact us](https://consensys.io/diligence/contact/).
</sub>


