import { NestFactory } from '@nestjs/core'
import { AuthModule } from './auth.module'
import { MicroserviceOptions, Transport } from '@nestjs/microservices'
import { ConfigService } from '@nestjs/config'
import { defaultNatsServerUrl, kNatsServerUrl } from '@server-octopus/shared'

async function bootstrap() {
    const app = await NestFactory.create(AuthModule)

    const microservice = app.connectMicroservice<MicroserviceOptions>({
        transport: Transport.NATS,
        options: {
            servers:
                app.get(ConfigService).get<string>(kNatsServerUrl) ??
                defaultNatsServerUrl
        }
    })
    await microservice.listen()
}
bootstrap()
