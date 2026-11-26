import type { Ref } from 'vue'
import { IR_RANKING_MAX_OFFSET } from '../../shared/constants/ir'
import type { IrRuleMode, LnScorePolicy } from '../../shared/types/ir'
import { type lnFilterItems, ruleModeItems } from '../utils/rankingDisplay'

export function useRankingQuery(
  lnPolicies: Ref<ReturnType<typeof lnFilterItems>>,
  { queryKey = 'ranking_page', pageSize = 100 } = {},
) {
  const route = useRoute()
  const pagination = useRoutePagination({
    pageSize,
    maxOffset: IR_RANKING_MAX_OFFSET,
    queryKey,
  })

  function setRankingFilter(key: 'ln_policy' | 'rule_mode' | 'scope', value?: string) {
    return navigateTo({
      path: route.path,
      query: { ...route.query, [key]: value, [queryKey]: undefined },
    })
  }

  const lnPolicy = computed<LnScorePolicy | 'ALL'>({
    get: () =>
      lnPolicies.value.find(({ value }) => value === route.query.ln_policy)?.value ?? 'ALL',
    set(value) {
      if (value !== lnPolicy.value) {
        void setRankingFilter('ln_policy', value === 'ALL' ? undefined : value)
      }
    },
  })
  const ruleMode = computed<IrRuleMode | 'ALL'>({
    get: () => ruleModeItems.find(({ value }) => value === route.query.rule_mode)?.value ?? 'ALL',
    set(value) {
      if (value !== ruleMode.value) {
        void setRankingFilter('rule_mode', value === 'ALL' ? undefined : value)
      }
    },
  })
  const ruleQuery = computed(() => ({
    ...(lnPolicy.value === 'ALL' ? {} : { ln_policy: lnPolicy.value }),
    ...(ruleMode.value === 'ALL' ? {} : { rule_mode: ruleMode.value }),
  }))
  const rankingQuery = computed(() => ({
    ...ruleQuery.value,
    limit: pagination.pageSize,
    offset: pagination.offset.value,
  }))

  return { lnPolicy, ruleMode, ruleQuery, rankingQuery, setRankingFilter, ...pagination }
}
