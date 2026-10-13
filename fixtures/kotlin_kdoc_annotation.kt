package x
private val JST: ZoneId = ZoneId.of("Asia/Tokyo")
/**
 * テスト用に出金申請を直接 INSERT する。申請者を省略するとストアの現オーナーになる。
 * 振込予定日は省略時、休業日調整なしの締め日ルールで決める。
 */
@Suppress("LongParameterList")
fun WithdrawalRequestRepository.createForTest(
    dsl: DSLContext,
    storeId: UUID,
    amount: Int = WithdrawalConstants.MIN_AMOUNT,
    source: WithdrawalRequestSource = WithdrawalRequestSource.MANUAL,
    requestedByUserId: UUID? = null,
    bankAccount: BankAccountSnapshot = testBankAccount(),
    requestedAt: OffsetDateTime = OffsetDateTime.now(),
    scheduledTransferDate: LocalDate =
        TransferSchedulePolicy.baseTransferDate(requestedAt.atZoneSameInstant(JST).toLocalDate()),
    transferFeeAmount: Int = WithdrawalConstants.TRANSFER_FEE_AMOUNT,
): WithdrawalRequest {
    val requester = requestedByUserId
        ?: dsl.select(STORES.OWNER_USER_ID)
            .from(STORES)
            .where(STORES.ID.eq(storeId))
            .fetchOne(STORES.OWNER_USER_ID)
        ?: error("Store owner not found for withdrawal request")
    return create(
        WithdrawalRequestCreation(
            storeId = storeId,
            requestedByUserId = requester,
            amount = amount,
            bankAccount = bankAccount,
            source = source,
            requestedAt = requestedAt,
            scheduledTransferDate = scheduledTransferDate,
            transferFeeAmount = transferFeeAmount,
            transferAmount = amount - transferFeeAmount,
        ),
    )
}

