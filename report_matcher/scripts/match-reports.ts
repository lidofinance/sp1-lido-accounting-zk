import dotenv from 'dotenv';
import { ethers } from 'ethers';

Object.defineProperties(BigInt.prototype, {
    toJSON: {
        value: function (this: BigInt) {
            return this.toString();
        },
    },
});

type EtherscanTx = {
    blockNumber: string;
    timeStamp: string;
    hash: string;
    nonce: string;
    blockHash: string;
    transactionIndex: string;
    from: string;
    to: string;
    value: string;
    gas: string;
    gasPrice: string;
    isError: string;
    txreceipt_status: string;
    input: string;
    contractAddress: string;
    cumulativeGasUsed: string;
    gasUsed: string;
    confirmations: string;
    methodId?: string;
    functionName?: string;
};

async function getContractTransactions(
    apiKey: string,
    contractAddress: string,
    startBlock = 0,
    endBlock = 99999999,
    chainId: number// 560048 for Hoodi
): Promise<EtherscanTx[]> {
    const baseUrl = `https://api.etherscan.io/v2/api`;

    const url = `${baseUrl}?chainid=${chainId}&module=account&action=txlist&address=${contractAddress}`
        + `&startblock=${startBlock}&endblock=${endBlock}&sort=asc&apikey=${apiKey}`;

    const res = await fetch(url);

    if (!res.ok) {
        throw new Error(`Etherscan API request failed with status ${res.status}`);
    }

    const data = await res.json();

    if (data.status !== "1" && data.message !== "No transactions found") {
        throw new Error(`Etherscan API error: ${data.message}`);
    }

    // Etherscan returns a string for each field
    const txs: EtherscanTx[] = data.result ?? [];

    // Filter strictly for transactions TO this contract (Etherscan also returns outgoing ones)
    return txs.filter(tx => tx.to?.toLowerCase() === contractAddress.toLowerCase());
}

type SubmitReportDataTx = {
    refSlot: bigint;
    numValidators: bigint;
    clBalanceGwei: bigint;
    withdrawalVaultBalance: bigint;
}

function parseAccountingOracleTx(tx: EtherscanTx): SubmitReportDataTx | null {
    try {
        const accountingOracleInterface = new ethers.Interface([
            // For V3
            "function submitReportData((uint256,uint256,uint256,uint256,uint256[],uint256[],uint256,uint256,uint256,uint256[],uint256,bool,bytes32,string,uint256,bytes32,uint256),uint256)",
            // For Triggerable Withdrawals?
            "function submitReportData((uint256,uint256,uint256,uint256,uint256[],uint256[],uint256,uint256,uint256,uint256[],uint256,bool,uint256,bytes32,uint256),uint256)"
        ]);

        const parsed = accountingOracleInterface.parseTransaction({
            data: tx.input,
            value: tx.value,
        });
        if (parsed.name == "submitReportData") {
            const submitReportDataTx: SubmitReportDataTx = {
                refSlot: parsed.args[0][1],
                numValidators: parsed.args[0][2],
                clBalanceGwei: parsed.args[0][3],
                withdrawalVaultBalance: parsed.args[0][6],
            }
            return submitReportDataTx;
        }
    } catch {
        // ignore unknown selectors / non - matching ABI
    }
    return null;
}

type ZkOracleTx = {
    refSlot: bigint;
    depositedLidoValidators: bigint;
    exitedLidoValidators: bigint;
    lidoClBalance: bigint;
    lidoWithdrawalVaultBalance: bigint;
}

function parseZkOracleTx(tx: EtherscanTx): ZkOracleTx | null {
    const iface = new ethers.Interface([
        "function submitReportData(bytes proof, bytes publicValues)"
    ]);

    try {
        const parsed = iface.parseTransaction({
            data: tx.input,
            value: tx.value,
        });
        if (parsed.name == "submitReportData") {
            const [publicValues] = ethers.AbiCoder.defaultAbiCoder().decode(
                ["(uint256,uint256,uint256,uint256,uint256,uint256,uint256,bytes32,bytes32,uint256,bytes32,uint256,bytes32,uint256,address)"],
                parsed.args[1]
            );

            const reportData: ZkOracleTx = {
                refSlot: publicValues[0],
                depositedLidoValidators: publicValues[1],
                exitedLidoValidators: publicValues[2],
                lidoClBalance: publicValues[3],
                lidoWithdrawalVaultBalance: publicValues[4],
            }
            return reportData;
        }
    } catch {
        // ignore unknown selectors / non-matching ABI
    }
    return null;
}

const runMain = async (): Promise<void> => {
    dotenv.config();

    const accountintOracleAddress = process.env.ACCOUNTING_ORACLE_ADDRESS || "";
    const ZK_ORACLE_ADDRESS = process.env.ZK_ORACLE_ADDRESS || "";
    const chainId = Number(process.env.CHAIN_ID) || 1;
    const API_KEY = process.env.ETHERSCAN_API_KEY || "";

    if (!API_KEY) throw new Error("Missing ETHERSCAN_API_KEY in env");

    const submitReportDataTxs: Map<bigint, SubmitReportDataTx> = new Map();
    const LOWER_BOUND = 0;

    try {
        const txs = await getContractTransactions(API_KEY, accountintOracleAddress, LOWER_BOUND, 99999999, chainId);
        console.log(`Found ${txs.length} txs to ${accountintOracleAddress}`);
        for (const tx of txs) {
            const date = new Date(Number(tx.timeStamp) * 1000).toISOString();
            console.log(`${tx.hash} | block ${tx.blockNumber} | ${date} | from ${tx.from} | value ${tx.value}`);
            const submitReportDataTx = parseAccountingOracleTx(tx);
            if (submitReportDataTx) {
                submitReportDataTxs.set(submitReportDataTx.refSlot, submitReportDataTx);
                console.log(`\nSubmit report data tx: ${JSON.stringify(submitReportDataTx)}`)
            }
        }

    } catch (err) {
        console.error("Error fetching transactions:", err);
    }

    console.log(`\nUseful reports: ${submitReportDataTxs.size}`)

    // ZK Report matching
    let noMatchingReports = 0;
    let depositedLidoValidatorsMismatch = 0;
    let lidoClBalanceMismatch = 0;
    let lidoWithdrawalVaultBalanceMismatch = 0;
    let foundReports = 0;
    try {
        const txs = await getContractTransactions(API_KEY, ZK_ORACLE_ADDRESS, 0, 99999999, chainId);
        console.log(`Found ${txs.length} txs to ${ZK_ORACLE_ADDRESS}`);
        for (const tx of txs) {
            const date = new Date(Number(tx.timeStamp) * 1000).toISOString();
            console.log(`${tx.hash} | block ${tx.blockNumber} | ${date} | from ${tx.from} | value ${tx.value}`);
            const zkOracleTx = parseZkOracleTx(tx);
            if (zkOracleTx) {
                foundReports++;
                console.log(`\nZK Oracle tx: ${JSON.stringify(zkOracleTx)}`)
                if (submitReportDataTxs.has(zkOracleTx.refSlot)) {
                    const tx = submitReportDataTxs.get(zkOracleTx.refSlot);
                    console.log(`\nSubmit report data tx: ${JSON.stringify(tx)}`)
                    if (tx) {
                        if (zkOracleTx.depositedLidoValidators != tx.numValidators) {
                            console.log(`\n[MISMATCH] Deposited lido validators mismatch: ${zkOracleTx.depositedLidoValidators} != ${tx.numValidators}`)
                            depositedLidoValidatorsMismatch++;
                        }
                        if (zkOracleTx.lidoClBalance != tx.clBalanceGwei) {
                            console.log(`\n[MISMATCH] Lido cl balance mismatch: ${zkOracleTx.lidoClBalance} != ${tx.clBalanceGwei}`)
                            lidoClBalanceMismatch++;
                        }
                        if (zkOracleTx.lidoWithdrawalVaultBalance != tx.withdrawalVaultBalance) {
                            console.log(`\n[MISMATCH] Lido withdrawal vault balance mismatch: ${zkOracleTx.lidoWithdrawalVaultBalance} != ${tx.withdrawalVaultBalance}`)
                            lidoWithdrawalVaultBalanceMismatch++;
                        }
                    }
                } else {
                    console.log(`\nNo submit report data tx for refSlot ${zkOracleTx.refSlot}`)
                    noMatchingReports++;
                }
            }
        }
    } catch (err) {
        console.error("Error fetching transactions:", err);
    }
    console.log(`\n--------------------------------------------------------`)
    console.log(`\nFound ZK reports: ${foundReports}`)
    console.log(`\nNot matching ZK reports: ${noMatchingReports}`)
    console.log(`\nDeposited lido validators mismatch: ${depositedLidoValidatorsMismatch}`)
    console.log(`\nLido cl balance mismatch: ${lidoClBalanceMismatch}`)
    console.log(`\nLido withdrawal vault balance mismatch: ${lidoWithdrawalVaultBalanceMismatch}`)
}

await runMain();
