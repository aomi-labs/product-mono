// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

import "@account-abstraction/interfaces/IAccount.sol";
import "@account-abstraction/interfaces/IEntryPoint.sol";
import "@account-abstraction/core/Helpers.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/**
 * @title SimpleAccount
 * @notice ERC-4337 v0.7 compatible smart contract account
 * @dev Owner-based account with ECDSA signature validation
 */
contract SimpleAccount is IAccount {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    address public owner;
    IEntryPoint private immutable _entryPoint;

    event SimpleAccountInitialized(IEntryPoint indexed entryPoint, address indexed owner);

    modifier onlyOwner() {
        require(msg.sender == owner, "SimpleAccount: not owner");
        _;
    }

    modifier onlyOwnerOrEntryPoint() {
        require(
            msg.sender == owner || msg.sender == address(_entryPoint),
            "SimpleAccount: not owner or entryPoint"
        );
        _;
    }

    constructor(IEntryPoint anEntryPoint) {
        _entryPoint = anEntryPoint;
        _disableInitializers();
    }

    function _disableInitializers() internal {
        // Prevent initialization on implementation contract
        owner = address(1);
    }

    /**
     * @notice Initialize the account with an owner
     * @param anOwner The owner address
     */
    function initialize(address anOwner) public virtual {
        require(owner == address(0), "SimpleAccount: already initialized");
        owner = anOwner;
        emit SimpleAccountInitialized(_entryPoint, owner);
    }

    /**
     * @notice Execute a single call
     * @param dest Target address
     * @param value ETH value
     * @param func Calldata
     */
    function execute(address dest, uint256 value, bytes calldata func) external onlyOwnerOrEntryPoint {
        _call(dest, value, func);
    }

    /**
     * @notice Execute a batch of calls
     * @param dest Array of target addresses
     * @param value Array of ETH values
     * @param func Array of calldatas
     */
    function executeBatch(
        address[] calldata dest,
        uint256[] calldata value,
        bytes[] calldata func
    ) external onlyOwnerOrEntryPoint {
        require(dest.length == func.length && dest.length == value.length, "SimpleAccount: wrong array lengths");
        for (uint256 i = 0; i < dest.length; i++) {
            _call(dest[i], value[i], func[i]);
        }
    }

    /**
     * @notice Validate user operation (ERC-4337 v0.7)
     * @param userOp The user operation
     * @param userOpHash Hash of the user operation
     * @param missingAccountFunds Missing account funds to pay to entry point
     * @return validationData Packed validation data (authorizer, validUntil, validAfter)
     */
    function validateUserOp(
        PackedUserOperation calldata userOp,
        bytes32 userOpHash,
        uint256 missingAccountFunds
    ) external virtual returns (uint256 validationData) {
        require(msg.sender == address(_entryPoint), "SimpleAccount: not from EntryPoint");

        // Validate signature
        validationData = _validateSignature(userOp, userOpHash);

        // Pay missing account funds
        _payPrefund(missingAccountFunds);
    }

    /**
     * @notice Get the entry point
     */
    function entryPoint() public view returns (IEntryPoint) {
        return _entryPoint;
    }

    /**
     * @notice Get the nonce from entry point
     */
    function getNonce() public view returns (uint256) {
        return _entryPoint.getNonce(address(this), 0);
    }

    /**
     * @notice Deposit more funds for this account in the entry point
     */
    function addDeposit() public payable {
        _entryPoint.depositTo{value: msg.value}(address(this));
    }

    /**
     * @notice Withdraw funds from entry point
     * @param withdrawAddress Target address
     * @param amount Amount to withdraw
     */
    function withdrawDepositTo(address payable withdrawAddress, uint256 amount) public onlyOwner {
        _entryPoint.withdrawTo(withdrawAddress, amount);
    }

    /**
     * @notice Get deposit info from entry point
     */
    function getDeposit() public view returns (uint256) {
        return _entryPoint.balanceOf(address(this));
    }

    /**
     * @dev Internal function to validate signature
     */
    function _validateSignature(PackedUserOperation calldata userOp, bytes32 userOpHash)
        internal
        virtual
        returns (uint256 validationData)
    {
        bytes32 hash = userOpHash.toEthSignedMessageHash();
        address recovered = hash.recover(userOp.signature);

        if (recovered != owner) {
            return SIG_VALIDATION_FAILED;
        }
        return SIG_VALIDATION_SUCCESS;
    }

    /**
     * @dev Internal function to pay prefund to entry point
     */
    function _payPrefund(uint256 missingAccountFunds) internal virtual {
        if (missingAccountFunds != 0) {
            (bool success,) = payable(msg.sender).call{value: missingAccountFunds, gas: type(uint256).max}("");
            (success); // Ignore failure (EntryPoint will revert if deposit is too low)
        }
    }

    /**
     * @dev Internal function to execute a call
     */
    function _call(address target, uint256 value, bytes memory data) internal {
        (bool success, bytes memory result) = target.call{value: value}(data);
        if (!success) {
            assembly {
                revert(add(result, 32), mload(result))
            }
        }
    }

    /**
     * @dev Receive function to accept ETH
     */
    receive() external payable {}
}
