export interface Result<T> {
    success: boolean
    message?: string
    data?: T
}

export class ResultUtil<T> implements Result<T> {
    data: T
    message: string
    success: boolean

    static ok<T>(data?: T): Result<T> {
        return {
            success: true,
            data
        }
    }

    static fail(): Result<any> {
        return {
            success: false
        }
    }

    static error(message: string): Result<any> {
        return {
            success: false,
            message
        }
    }

    isOk(): boolean {
        return this.success
    }
}
