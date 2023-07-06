import { Controller } from '@nestjs/common'
import { MessagePattern } from '@nestjs/microservices'

@Controller()
export class MongoController {
    @MessagePattern({ cmd: 'sum' })
    accumulate(data: number[]): number {
        return (data || []).reduce((a, b) => a + b)
    }
}
