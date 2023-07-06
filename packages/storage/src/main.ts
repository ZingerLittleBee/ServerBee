import { NestFactory } from '@nestjs/core'
import { StorageModule } from './storage.module'
import { MicroserviceOptions, Transport } from '@nestjs/microservices'

async function bootstrap() {
    const app = await NestFactory.createMicroservice<MicroserviceOptions>(
        StorageModule,
        {
            transport: Transport.NATS,
            options: {
                servers: ['nats://localhost:4222']
            }
        }
    )
    await app.listen()
}
bootstrap()
