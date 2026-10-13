package com.example.support

import com.example.domain.constant.OrderConstants
import com.example.domain.model.AccountSnapshot
import com.example.domain.model.AccountType
import com.example.domain.model.Order
import com.example.domain.model.OrderCreation
import com.example.domain.model.OrderSource
import com.example.domain.model.UserAccount
import com.example.domain.repository.OrderRepository
import com.example.domain.repository.UserAccountRepository
import com.example.domain.service.SchedulePolicy
import com.example.jooq.Tables.SHOPS
import org.jooq.DSLContext
import java.time.LocalDate
import java.time.OffsetDateTime
import java.time.ZoneId
import java.util.UUID

private val TOKYO: ZoneId = ZoneId.of("Asia/Tokyo")

/** テスト用のアカウントスナップショット（外部サービス参照＋表示用情報）。 */
fun testAccount(
    externalId: Long = 123456L,
    externalCode: String = "test-partner",
    displayName: String = "Test Name",
    branchName: String = "Test Branch",
    accountType: AccountType = AccountType.SAVINGS,
    numberLast4: String = "4567",
): AccountSnapshot = AccountSnapshot(
    externalId = externalId,
    externalCode = externalCode,
    displayName = displayName,
    branchName = branchName,
    accountType = accountType,
    numberLast4 = numberLast4,
)

/** テスト用に、ユーザーの登録済みアカウントを外部サービスを呼ばずに直接保存する。 */
fun UserAccountRepository.registerForTest(
    userId: UUID,
    account: AccountSnapshot = testAccount(externalCode = "test-$userId"),
): UserAccount = upsert(userId, account)

/**
 * テスト用に注文を直接 INSERT する。依頼者を省略するとショップの現オーナーになる。
 * 実行予定日は省略時、休業日調整なしの締め日ルールで決める。
 */
@Suppress("LongParameterList")
fun OrderRepository.createForTest(
    dsl: DSLContext,
    shopId: UUID,
    amount: Int = OrderConstants.MIN_AMOUNT,
    source: OrderSource = OrderSource.MANUAL,
    requestedByUserId: UUID? = null,
    account: AccountSnapshot = testAccount(),
    requestedAt: OffsetDateTime = OffsetDateTime.now(),
    scheduledDate: LocalDate =
        SchedulePolicy.baseDate(requestedAt.atZoneSameInstant(TOKYO).toLocalDate()),
    feeAmount: Int = OrderConstants.FEE_AMOUNT,
): Order {
    val requester = requestedByUserId
        ?: dsl.select(SHOPS.OWNER_USER_ID)
            .from(SHOPS)
            .where(SHOPS.ID.eq(shopId))
            .fetchOne(SHOPS.OWNER_USER_ID)
        ?: error("Shop owner not found for order")
    return create(
        OrderCreation(
            shopId = shopId,
            requestedByUserId = requester,
            amount = amount,
            account = account,
            source = source,
            requestedAt = requestedAt,
            scheduledDate = scheduledDate,
            feeAmount = feeAmount,
            netAmount = amount - feeAmount,
        ),
    )
}
