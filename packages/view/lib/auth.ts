export async function isAuthenticated(cookie: string): Promise<boolean> {
  const response = await fetch("http://localhost:3001/auth/check", {
    method: "GET",
    headers: { "Content-Type": "application/json", Cookie: cookie }
  })
  console.log('response ok', response.ok)
  return response.ok
}
