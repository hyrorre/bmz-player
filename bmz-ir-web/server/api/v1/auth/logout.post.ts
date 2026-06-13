export default defineEventHandler(async (event) => {
  await clearUserSession(event)

  return { logged_out: true }
})
