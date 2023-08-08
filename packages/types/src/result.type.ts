export interface Result<T = any> {
    success: boolean
    message?: string
    data?: T
}
